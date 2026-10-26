#!/usr/bin/env python3
"""Record active, coached Flik demonstrations using real PTY sessions.

Build Flik first, then install Pillow, pyte and imageio-ffmpeg and run this file.
Only public synthetic fixtures are recorded; never the desktop, user logs, or a shell.
An optional --font accepts a monospace font on platforms without Menlo/DejaVu.
"""
from __future__ import annotations
import argparse
import codecs
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import shutil
import struct
import subprocess
import tempfile
import termios
import time

from PIL import Image, ImageDraw, ImageFont
import pyte
import imageio_ffmpeg

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'website/dist/assets/demos'
FIXTURES = ROOT / 'website/demo-data'
WIDTH, HEIGHT, COLS, ROWS, FPS = 1280, 960, 100, 40, 10
X, Y, CELL_W, CELL_H = 40, 78, 12, 18
PALETTE = {'black':'10101b','red':'ff6c7c','green':'84e9bb','brown':'f5d883',
    'blue':'8aa8ff','magenta':'c3a2ff','cyan':'8ae5ea','white':'edf0ff',
    'brightblack':'666879','brightred':'ff8995','brightgreen':'a7ffd3',
    'brightbrown':'ffe69c','brightblue':'b3c1ff','brightmagenta':'dac5ff',
    'brightcyan':'adffff','brightwhite':'ffffff'}


def make_fixtures():
    FIXTURES.mkdir(parents=True, exist_ok=True)
    messages = [('INFO','gateway ready'),('INFO','configuration loaded'),
        ('INFO','worker started'),('INFO','cache warmup complete'),
        ('INFO','request req-demo-01 accepted'),('INFO','request completed in 24ms'),
        ('INFO','request req-demo-02 accepted'),('INFO','reading job queue'),
        ('NOTICE','queue depth: 8'),('INFO','opening upstream connection'),
        ('WARN','upstream response delayed'),('ERROR','upstream timeout; request req-demo-02'),
        ('INFO','retry scheduled'),('INFO','retry completed in 30ms'),
        ('INFO','request req-demo-03 accepted'),('INFO','request completed in 18ms'),
        ('INFO','request req-demo-04 accepted'),('INFO','loading catalog'),
        ('WARN','cache entry expired'),('INFO','opening cache connection'),
        ('WARN','cache pool at capacity'),('ERROR','cache timeout; request req-demo-04'),
        ('INFO','connection released'),('INFO','request retried successfully'),('INFO','worker heartbeat')]
    (FIXTURES/'service.log').write_text(''.join(f'2026-01-12T09:00:{i:02d}Z {level:<6} {message}\n' for i,(level,message) in enumerate(messages)))
    events=[]
    for i in range(12):
        service='payment' if i % 3 else 'catalog'
        level='error' if i % 4 == 1 else 'info'
        event={'level':level,'service':service,'message':'Gateway timeout' if level=='error' else 'Payment accepted',
            'request_id':f'req-demo-{i:02d}','payload':{'amount':42+i,'currency':'USD',
            'gateway':{'region':'test-west','status':'timeout' if level=='error' else 'ready'}}}
        events.append(f'2026-01-12T09:00:{i:02d}.000Z stdout F '+json.dumps(event,separators=(',',':'))+'\n')
    (FIXTURES/'kubernetes.log').write_text(''.join(events))
    events=[]
    for hour,count in enumerate([2,3,2,5,8,13,21,12,7,4,2,1],start=7):
        events.extend(f'2026-01-12T{hour:02d}:{i:02d}:00Z ERROR upstream timeout\n' for i in range(count))
        events.append(f'2026-01-12T{hour:02d}:59:00Z INFO worker heartbeat\n')
    (FIXTURES/'hourly.log').write_text(''.join(events))


def color(value,fallback):
    if value=='default': return fallback
    value=PALETTE.get(value,value)
    try: return tuple(int(value[i:i+2],16) for i in (0,2,4)) if len(value)==6 else fallback
    except ValueError: return fallback


def find_font(requested):
    if requested: return requested
    for path in ['/System/Library/Fonts/Menlo.ttc','/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf']:
        if Path(path).exists(): return path
    raise SystemExit('Pass --font /path/to/monospace.ttf')


def stamp(seconds):
    ms=round(seconds*1000)
    return f'{ms//3600000:02}:{ms//60000%60:02}:{ms//1000%60:02}.{ms%1000:03}'


class Recording:
    def __init__(self,name,filename,title,font,workspace,expected=21):
        self.name,self.title,self.expected=name,title,expected
        self.font=ImageFont.truetype(font,16)
        self.small=ImageFont.truetype(font,15)
        self.big=ImageFont.truetype(font,24)
        self.screen=pyte.Screen(COLS,ROWS)
        self.stream=pyte.Stream(self.screen)
        self.decoder=codecs.getincrementaldecoder('utf-8')('replace')
        self.master,slave=pty.openpty()
        fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',ROWS,COLS,0,0))
        self.process=subprocess.Popen([str(ROOT/'target/debug/flik'),filename],cwd=workspace,
            stdin=slave,stdout=slave,stderr=slave,
            env={'PATH':os.environ.get('PATH','/usr/bin:/bin'),'TERM':'xterm-256color','LANG':'en_US.UTF-8'})
        os.close(slave)
        self.frames=0
        self.steps=[]
        self.coach=('START','Open a synthetic log','Every action below is performed in the real Flik app.')
        self.terminal=Image.new('RGB',(WIDTH,HEIGHT),'#22243b')
        self.encoder=subprocess.Popen([imageio_ffmpeg.get_ffmpeg_exe(),'-hide_banner','-loglevel','error','-y',
            '-f','rawvideo','-pix_fmt','rgb24','-s',f'{WIDTH}x{HEIGHT}','-r',str(FPS),'-i','-',
            '-an','-c:v','libx264','-preset','fast','-crf','22','-pix_fmt','yuv420p','-movflags','+faststart',
            str(OUT/f'{name}.mp4')],stdin=subprocess.PIPE)
        self.contact=[]
        self.drain(.6)

    def drain(self,duration=0):
        end=time.monotonic()+duration
        while True:
            if select.select([self.master],[],[],max(0,min(.025,end-time.monotonic())))[0]:
                try: data=os.read(self.master,1<<18)
                except OSError: break
                self.stream.feed(self.decoder.decode(data))
            if time.monotonic()>=end: break

    def step(self,key,title,detail):
        if self.steps: self.steps[-1]['end']=self.frames/FPS
        self.steps.append({'start':self.frames/FPS,'key':key,'title':title,'detail':detail})
        self.coach=(key,title,detail)

    def screen_text(self): return '\n'.join(self.screen.display)

    def frame(self):
        # Render only changed terminal rows; progress and coaching remain animated.
        draw=ImageDraw.Draw(self.terminal)
        for row in sorted(self.screen.dirty):
            for column in range(COLS):
                cell=self.screen.buffer[row][column]
                fg,bg=color(cell.fg,(226,232,240)),color(cell.bg,(15,23,42))
                if cell.reverse: fg,bg=bg,fg
                cx,cy=X+column*CELL_W,Y+row*CELL_H
                draw.rectangle((cx,cy,cx+CELL_W-1,cy+CELL_H-1),fill=bg)
                if cell.data: draw.text((cx,cy-2),cell.data,font=self.font,fill=fg)
        self.screen.dirty.clear()
        im=self.terminal.copy()
        d=ImageDraw.Draw(im)
        d.text((40,22),'flik / '+self.title,font=self.font,fill='#f3f5f8')
        d.text((1000,24),'SYNTHETIC DATA',font=self.small,fill='#9daeca')
        d.rounded_rectangle((32,814,1248,928),radius=12,fill='#f3f5f8')
        key,title,detail=self.coach
        d.rounded_rectangle((49,832,211,909),radius=9,fill='#24243e')
        key_size=self.big.getlength(key)
        d.text((130-key_size/2,851),key,font=self.big,fill='#ffd08a')
        d.text((235,832),title,font=self.big,fill='#24243e')
        d.text((236,872),detail,font=self.small,fill='#53627a')
        d.text((1160,839),f'{len(self.steps):02d}',font=self.big,fill='#667993')
        progress=min(1,(self.frames+1)/FPS/self.expected)
        d.rounded_rectangle((40,943,1240,948),radius=2,fill='#45526b')
        d.rounded_rectangle((40,943,40+int(1200*progress),948),radius=2,fill='#ffc778')
        self.encoder.stdin.write(im.tobytes())
        self.frames+=1
        return im

    def hold(self,seconds,poster=False,contact=False):
        assert seconds<=2,'Do not leave long static holds in coached demos'
        last=None
        for _ in range(round(seconds*FPS)):
            self.drain(.025)
            last=self.frame()
        if poster and last: last.save(OUT/f'{self.name}.png',optimize=True)
        if contact and last: self.contact.append(last.copy())

    def send(self,data,seconds=.3):
        os.write(self.master,data)
        self.hold(seconds)

    def type(self,text):
        for char in text:
            os.write(self.master,char.encode())
            self.drain(.045)
            self.frame()

    def navigate(self,key,count=3):
        for _ in range(count): self.send(key,.25)

    def finish(self):
        if self.steps: self.steps[-1]['end']=self.frames/FPS
        self.encoder.stdin.close()
        if self.encoder.wait(timeout=60): raise RuntimeError('Encoding failed')
        os.write(self.master,b'\x1b')
        self.drain(.1)
        os.write(self.master,b'Q')
        try: self.process.wait(timeout=3)
        except subprocess.TimeoutExpired: self.process.kill();self.process.wait()
        os.close(self.master)
        vtt='WEBVTT\n\n'+'\n\n'.join(f"{stamp(s['start'])} --> {stamp(s['end'])}\n{s['title']}\n{s['detail']}" for s in self.steps)+'\n'
        for suffix in ['vtt','en.vtt']: (OUT/f'{self.name}.{suffix}').write_text(vtt)
        (OUT/f'{self.name}.steps.json').write_text(json.dumps(self.steps,indent=2)+'\n')
        if self.contact:
            thumbs=[]
            for frame in self.contact:
                frame.thumbnail((512,360))
                thumbs.append(frame)
            sheet=Image.new('RGB',(1024,360*((len(thumbs)+1)//2)),'#22243b')
            for i,frame in enumerate(thumbs): sheet.paste(frame,((i%2)*512,(i//2)*360))
            sheet.save(Path('/tmp')/f'flik-{self.name}-contact.jpg',quality=90)
        return {'file':f'{self.name}.mp4','poster':f'{self.name}.png','captions':f'{self.name}.vtt',
            'steps':f'{self.name}.steps.json','duration':self.frames/FPS,'width':WIDTH,'height':HEIGHT,
            'bytes':(OUT/f'{self.name}.mp4').stat().st_size,'actions':len(self.steps)}


def record_kubernetes(font,workspace):
    r=Recording('kubernetes-json','kubernetes.log','Kubernetes JSON, under control',font,workspace,17.8)
    r.step('g','Open the compact Kubernetes log','Real CRI records with a nested JSON payload.')
    r.send(b'g',.4)
    r.step('w','Press w — make JSON readable','Expand the nested payload directly inside the log view.')
    r.send(b'w',.5);r.hold(1.1,contact=True)
    assert 'gateway' in r.screen_text(),'JSON did not expand'
    r.step('↓ ↑','Move through the expanded events','Keep the original timestamp and structured fields in view.')
    r.navigate(b'\x1b[B',3);r.navigate(b'\x1b[A',2);r.hold(.6)
    r.step('6','Press 6 — inspect the detected fields','Structured field selection is available without rewriting the log.')
    r.send(b'6',.5);r.navigate(b'\x1b[B',2);r.hold(.6,contact=True);r.send(b'\x1b',.4)
    r.step('3','Type service:payment','Add an include filter; keep only payment events.')
    r.send(b'3',.4);r.type('service:payment');r.hold(.5,contact=True);r.send(b'\r',.6);r.hold(.8)
    assert 'service:payment' in r.screen_text(),'Payment filter not visible'
    r.step('3','Type level:error','Stack a second include filter on top of service:payment.')
    r.send(b'3',.4);r.type('level:error');r.hold(.5);r.send(b'\r',.7)
    r.hold(1.1,poster=True,contact=True)
    assert 'level:error' in r.screen_text(),'Second filter not visible'
    r.step('x','Type x — remove last filter','Undo level:error; keep the broader service:payment filter.')
    r.send(b'x',.6);r.hold(1.2,contact=True)
    r.step('w','Press w — switch back to compact','The filters stay active while the JSON view changes.')
    r.send(b'w',.5);r.hold(.9,contact=True)
    r.step('w','Press w — expand the payload again','From compact records to readable JSON in one keystroke.')
    r.send(b'w',.5);r.hold(1.0)
    return r.finish()


def record_smart(font,workspace):
    r=Recording('smart-find','service.log','Search, stack filters, undo',font,workspace,18.7)
    r.step('/','Type //3,2:error','Find errors with three lines before and two lines after.')
    r.hold(.3);r.type('//3,2:error');r.hold(.6,contact=True);r.send(b'\r',.5);r.hold(1.1,poster=True,contact=True)
    r.step('↑ ↓','Browse the matching events','Gray lines provide context; the error stays highlighted.')
    r.navigate(b'\x1b[A',3);r.navigate(b'\x1b[B',2);r.hold(.7)
    r.step('x','Type x — remove last filter','Undo the smart find and return to the full log.')
    r.send(b'x',.5);r.hold(1.1,contact=True)
    r.step('3','Type timeout','Add a focused include filter for timeout events.')
    r.send(b'3',.4);r.type('timeout');r.hold(.5);r.send(b'\r',.5);r.hold(.9,contact=True)
    r.step('8','Type cache — exclude noisy matches','Stack an exclude filter without losing the timeout filter.')
    r.send(b'8',.4);r.type('cache');r.hold(.6,contact=True);r.send(b'\r',.5);r.hold(1.0)
    r.step('x','Type x — remove last filter','Remove only the cache exclusion; both timeout events return.')
    r.send(b'x',.6);r.hold(1.0,contact=True)
    r.step('Tab','Press Tab — inspect structured columns','The same active filter can be viewed as a table.')
    r.send(b'\t',.6);r.hold(.8);r.send(b'\t',.5)
    r.step('x','Type x — restore the full log','Remove timeout and resume browsing all events.')
    r.send(b'x',.6);r.send(b'g',.5);r.navigate(b'\x1b[B',3);r.hold(.6)
    return r.finish()


def record_hourly(font,workspace):
    r=Recording('hourly-frequency','hourly.log','Find the spike, change the interval',font,workspace,17.7)
    r.step('/','Type /error','Start with a focused search for error events.')
    r.hold(.3);r.type('/error');r.hold(.4);r.send(b'\r',.5);r.hold(.7,contact=True)
    r.step('H','Press H — count matches by hour','Exact counts across the indexed file; context is excluded.')
    r.send(b'H',.6);r.hold(1.1,poster=True,contact=True)
    assert 'Occurrences over time' in r.screen_text(),'Hourly report did not open'
    r.step('↓ ↑','Scroll the hourly report','Read the dates, UTC hours, counts and distribution.')
    r.navigate(b'\x1b[B',3);r.navigate(b'\x1b[A',2);r.hold(.6)
    r.step('Esc','Press Esc — return to the filtered log','Keep the error filter while changing the time interval.')
    r.send(b'\x1b',.5);r.hold(.7)
    r.step('Ctrl+K','Type histogram minute','Choose one-minute buckets from the command dialog.')
    r.send(b'\x0b',.4);r.type('histogram minute');r.hold(.6,contact=True);r.send(b'\r',.6);r.hold(.8)
    r.step('PgDn','Press Page Down — explore minute buckets','Scroll through the finer-grained distribution.')
    r.send(b'\x1b[6~',.5);r.hold(.7,contact=True);r.send(b'\x1b[6~',.5);r.hold(.6)
    r.step('Esc','Press Esc — keep the filter, close the report','Return to the source events without rebuilding the query.')
    r.send(b'\x1b',.5);r.hold(.7)
    r.step('x','Type x — remove last filter','Undo the search and restore every event.')
    r.send(b'x',.5);r.hold(.7,contact=True)
    r.step('g ↓','Browse the restored log','The original data remains unchanged throughout the investigation.')
    r.send(b'g',.4);r.navigate(b'\x1b[B',4);r.hold(.8)
    return r.finish()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--font')
    args=parser.parse_args()
    font=find_font(args.font)
    OUT.mkdir(parents=True,exist_ok=True)
    make_fixtures()
    old=[]
    if (OUT/'manifest.json').exists():
        old=[v for v in json.loads((OUT/'manifest.json').read_text()).get('videos',[]) if v['file']=='apache-levels.mp4']
    with tempfile.TemporaryDirectory(prefix='flik-demo-',dir='/tmp') as temporary:
        workspace=Path(temporary)
        for fixture in FIXTURES.glob('*.log'): shutil.copyfile(fixture,workspace/fixture.name)
        videos=[record_kubernetes(font,workspace),record_smart(font,workspace),record_hourly(font,workspace)]
    manifest={'synthetic_data_only':True,'language':'en','capture':'Real Flik PTY sessions with burned-in hotkey coaching',
        'videos':videos+old}
    (OUT/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(json.dumps(manifest,indent=2))

if __name__=='__main__': main()
