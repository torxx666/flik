#!/usr/bin/env python3
"""Capture real Flik 0.3.0 terminal buffers using synthetic, downloadable inputs."""
import argparse
import shutil
import codecs,fcntl,json,os,pty,select,struct,subprocess,tempfile,termios,time
from pathlib import Path
from PIL import Image,ImageDraw,ImageFont
import pyte
from generate_site_demos import color

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'website/dist/assets/screens';OUT.mkdir(parents=True,exist_ok=True)
SAMPLES=ROOT/'website/dist/assets/samples/incident';SAMPLES.mkdir(parents=True,exist_ok=True)
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--binary',type=Path,default=ROOT/'target/release/flik');args=parser.parse_args()
BINARY=args.binary.resolve()
assert subprocess.check_output([str(BINARY),'--version'],text=True).strip()=='flik 0.3.0'
files={'api':[],'worker':[],'auth':[]}
for i in range(60):
 service=['auth','api','worker'][i%3]
 message={'auth':'token valid','api':'request accepted','worker':'job started'}[service]
 level='INFO'
 if i==52:message='upstream timeout';level='ERROR'
 if i==56:message='retry scheduled';level='WARN'
 if i==59:message='retry completed'
 files[service].append(f'2026-11-12T10:00:{i:02d}Z {level} req-42 {message}')
for name,lines in files.items():(SAMPLES/(name+'.log')).write_text('\n'.join(lines)+'\n')
shutil.copyfile(ROOT/'website/demo-data/kubernetes.log',SAMPLES/'kubernetes.log')
manifest=[]
def capture(name,args,cols,rows,title,keys=b'',must=()):
 with tempfile.TemporaryDirectory(prefix='flik-screen-',dir='/tmp') as directory:
  master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',rows,cols,0,0))
  env={**os.environ,'TERM':'xterm-256color','XDG_CONFIG_HOME':directory,'TMPDIR':directory,'NO_COLOR':''}
  proc=subprocess.Popen([str(BINARY),*args],cwd=SAMPLES,stdin=slave,stdout=slave,stderr=slave,env=env);os.close(slave)
  screen=pyte.Screen(cols,rows);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf-8')('replace')
  def pump(seconds):
   end=time.monotonic()+seconds
   while time.monotonic()<end:
    if select.select([master],[],[],.05)[0]:
     try:stream.feed(decoder.decode(os.read(master,262144)))
     except OSError:break
  try:
   pump(1.2)
   if keys:os.write(master,keys);pump(.6)
   text='\n'.join(screen.display)
   for needle in must:assert needle in text,(name,needle,text)
   cw,ch,pad,top=12,24,24,76
   image=Image.new('RGB',(cols*cw+pad*2,rows*ch+top+pad),'#171b2b');draw=ImageDraw.Draw(image)
   font=ImageFont.truetype(str(ROOT/'website/dist/assets/ibm-plex-mono-latin.ttf'),18)
   draw.text((pad,18),title,font=font,fill='#f0f2ff')
   draw.text((pad,43),'Flik 0.3.0 · real terminal capture · synthetic data',font=ImageFont.truetype(str(ROOT/'website/dist/assets/ibm-plex-mono-latin.ttf'),14),fill='#b7bfd7')
   for y in range(rows):
    for x in range(cols):
     cell=screen.buffer[y][x];fg=color(cell.fg,(226,232,240));bg=color(cell.bg,(15,23,42))
     if cell.reverse:fg,bg=bg,fg
     xx,yy=pad+x*cw,top+y*ch
     draw.rectangle((xx,yy,xx+cw-1,yy+ch-1),fill=bg)
     if cell.data:draw.text((xx,yy),cell.data,font=font,fill=fg)
   image.save(OUT/(name+'.png'),optimize=True)
   image.save(OUT/(name+'.webp'),quality=92)
   manifest.append({'name':name,'command':'flik '+' '.join(args),'keys':keys.decode(),'columns':cols,'rows':rows,'verified_visible':list(must),'version':'0.3.0','synthetic':True})
  finally:
   os.write(master,b'\x1bQ');pump(.15)
   if proc.poll() is None:proc.terminate()
   proc.wait(timeout=5);os.close(master)
capture('live-merge',['-m','api.log','worker.log','auth.log'],120,28,'Follow one incident across three local services',must=('api.log','worker.log','auth.log','retry scheduled'))
capture('long-split',['-m','-l','api.log','worker.log','auth.log'],180,26,'Three consecutive pages: previous history and current events',must=('LONG-SPLIT','p-2','p-1','Current'))
capture('json-inspector',['kubernetes.log'],115,30,'Inspect a structured Kubernetes event',keys=b'J',must=('JSON','gateway'))
(OUT/'captures.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Captured three actual Flik 0.3.0 screens with verified visible content.')
