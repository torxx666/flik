#!/usr/bin/env python3
"""Deploy only dist/ to an existing Nginx host: python3 website/scripts/deploy_server.py user@host.
The host must already terminate public HTTPS at its edge proxy and allow root deployment.
"""
from pathlib import Path
import argparse,gzip,hashlib,io,json,re,subprocess,sys,tarfile,tempfile
ROOT=Path(__file__).resolve().parents[2]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('server',help='SSH destination, user@host')
parser.add_argument('--check-host',action='append',default=[],help='Existing origin hostname to health-check')
parser.add_argument('--check-static-host',action='append',default=[],help='Existing static hostname whose page must remain identical')
args=parser.parse_args()
server=args.server
if not re.fullmatch(r'[A-Za-z0-9_.-]+@[A-Za-z0-9_.-]+',server):
    parser.error('SSH destination must be user@host')
for host in args.check_host+args.check_static_host:
    if not re.fullmatch(r'[A-Za-z0-9.-]+',host):parser.error('Invalid health-check hostname')
subprocess.run([sys.executable,str(ROOT/'website/scripts/check_site.py')],check=True)
site=ROOT/'website/dist'
with tempfile.TemporaryDirectory(prefix='flik-web-deploy-') as temporary:
    package=Path(temporary)/'site.tar.gz'
    with package.open('wb') as output,gzip.GzipFile(fileobj=output,mode='wb',mtime=0,filename='') as compressed,tarfile.open(fileobj=compressed,mode='w') as tar:
        for file in sorted(site.rglob('*')):
            if not file.is_file():continue
            assert not file.is_symlink()
            data=file.read_bytes();info=tarfile.TarInfo(file.relative_to(site).as_posix());info.size=len(data);info.mode=0o644
            tar.addfile(info,io.BytesIO(data))
    digest=hashlib.sha256(package.read_bytes()).hexdigest()
    release='site-'+digest[:16]
    remote='/tmp/flik-web-'+digest[:16]+'.tar.gz'
    options=['-o','BatchMode=yes','-o','ConnectTimeout=10','-o','StrictHostKeyChecking=yes']
    subprocess.run(['scp',*options,str(package),server+':'+remote],check=True)
    parameters={'archive':remote,'digest':digest,'release':release,'config':(ROOT/'website/deploy/nginx.conf').read_text(),'index_sha256':hashlib.sha256((site/'index.html').read_bytes()).hexdigest(),'neighbors':list(dict.fromkeys(args.check_host+args.check_static_host)),'static_neighbors':args.check_static_host}
    code='PARAMETERS='+repr(parameters)+'\n'+r'''
from pathlib import Path
import hashlib,json,os,subprocess,tarfile,time,urllib.request
p=PARAMETERS
archive=Path(p['archive'])
assert hashlib.sha256(archive.read_bytes()).hexdigest()==p['digest'],'Upload checksum mismatch'
base=Path('/var/www/flik');release=base/'releases'/p['release'];current=base/'current'
config=Path('/etc/nginx/conf.d/flik.conf')
if current.exists() and not current.is_symlink():raise RuntimeError('Current path is not a deployment symlink')
old_link=os.readlink(current) if current.is_symlink() else None
old_config=config.read_bytes() if config.exists() else None
if old_config is not None and b'server_name flik.spedo.dev;' not in old_config:
    raise RuntimeError('Refusing to replace an unrelated Nginx configuration')
def fetch(host,path='/',headers=None):
    request=urllib.request.Request('http://127.0.0.1'+path,headers={'Host':host,**(headers or {})})
    with urllib.request.urlopen(request,timeout=15) as response:return response.status,response.read(),dict(response.headers)
before={host:fetch(host) for host in p['neighbors']}
release.mkdir(parents=True,exist_ok=True)
with tarfile.open(archive,'r:gz') as tar:
    for member in tar:
        path=Path(member.name)
        if not member.isfile() or path.is_absolute() or '..' in path.parts:raise RuntimeError('Unexpected archive member')
        destination=release/path;destination.parent.mkdir(parents=True,exist_ok=True)
        destination.write_bytes(tar.extractfile(member).read());destination.chmod(0o644)
assert hashlib.sha256((release/'index.html').read_bytes()).hexdigest()==p['index_sha256']
assert b'<html lang="en">' in (release/'index.html').read_bytes()
backups=base/'deployments';backups.mkdir(exist_ok=True)
if old_config is not None:(backups/(p['release']+'.previous-nginx.txt')).write_bytes(old_config)
def point(target):
    temporary=base/'.current-next'
    temporary.unlink(missing_ok=True);temporary.symlink_to(target);os.replace(temporary,current)
try:
    # Preserve live first-party collection and any operator-owned vhost settings.
    if old_config is None:
        temporary=config.with_name('.flik.conf.new');temporary.write_text(p['config']);temporary.chmod(0o644);os.replace(temporary,config)
    point(release)
    subprocess.run(['nginx','-t'],check=True)
    subprocess.run(['systemctl','reload','nginx'],check=True)
    deadline=time.monotonic()+10
    while True:
        status,body,headers=fetch('flik.spedo.dev')
        if status==200 and hashlib.sha256(body.replace(b'<script defer src="/_tracer/collector.js"></script>',b'')).hexdigest()==p['index_sha256']:break
        if time.monotonic()>=deadline:raise RuntimeError('Incorrect Flik page after Nginx reload')
        time.sleep(0.2)
    status,body,headers=fetch('flik.spedo.dev','/site-config.json')
    assert status==200 and json.loads(body)['repository']=='torxx666/flik'
    status,body,headers=fetch('flik.spedo.dev','/assets/demos/kubernetes-json.mp4',{'Range':'bytes=0-1023'})
    assert status==206 and len(body)==1024 and headers.get('Content-Type')=='video/mp4','Video byte ranges are unavailable'
    after={host:fetch(host) for host in before}
    assert all(after[host][0]==before[host][0]==200 for host in before),'A neighboring site health check failed'
    for host in p['static_neighbors']:
        assert hashlib.sha256(before[host][1]).digest()==hashlib.sha256(after[host][1]).digest(),'An existing static site changed'
except Exception:
    if old_config is None:config.unlink(missing_ok=True)
    else:config.write_bytes(old_config)
    if old_link is None:current.unlink(missing_ok=True)
    else:point(old_link)
    subprocess.run(['nginx','-t'],check=True)
    subprocess.run(['systemctl','reload','nginx'],check=True)
    raise
state={'release':p['release'],'root':str(release),'previous':old_link,'index_sha256':p['index_sha256'],'checks':['Nginx syntax','Flik page hash','GitHub repository configuration','MP4 byte ranges','Neighbor HTTP 200','Unchanged static neighbor content']}
(backups/(p['release']+'.json')).write_text(json.dumps(state,indent=2)+'\n')
archive.unlink()
print(json.dumps(state))
'''
    result=subprocess.run(['ssh',*options,server,'python3 -'],input=code,text=True,capture_output=True)
    sys.stdout.write(result.stdout);sys.stderr.write(result.stderr)
    result.check_returncode()
    artifacts=ROOT/'artifacts';artifacts.mkdir(exist_ok=True)
    (artifacts/'server-deployment.json').write_text(result.stdout)
