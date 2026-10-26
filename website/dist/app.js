'use strict';
const demos = {
  "kubernetes-json": {
    "number": "01",
    "label": "flik · Kubernetes JSON",
    "name": "Read the JSON. Narrow the failure. Undo one step.",
    "description": "Press w to expand embedded Kubernetes JSON. Add service:payment, add level:error, then press x to remove only the last filter.",
    "transcript": "The real recording opens compact CRI logs, beautifies the embedded JSON, stacks two include rules and removes the newest rule. The first rule stays active.",
    "command": "w → 3 → service:payment → 3 → level:error → x"
  },
  "smart-find": {
    "number": "02",
    "label": "flik · context and filter stack",
    "name": "Follow the context. Refine the matches. Step back.",
    "description": "Search //3,2:error, inspect surrounding events, then use include and exclude rules to narrow the results. Press x to undo the most recent rule.",
    "transcript": "Watch the filter stack and the visible events change together. On-screen prompts show the exact keys being typed.",
    "command": "//3,2:error → x → 3 → 8 → x"
  },
  "hourly-frequency": {
    "number": "03",
    "label": "flik · error frequency",
    "name": "Filter the error. Find the hour. Change the interval.",
    "description": "Search for errors, press H for hourly counts and navigate the report. Open Ctrl+K to inspect a different time interval.",
    "transcript": "Counts scan the indexed file with the active filters. Context-only lines are excluded and timestamps are shown in UTC.",
    "command": "/error → H → Ctrl+K → histogram minute"
  }
};
let activeDemo='kubernetes-json', selectedTarget='aarch64-apple-darwin', repository=null, release=null;
const $=id=>document.getElementById(id);
function lineText(el,text){el.textContent=text;el.style.whiteSpace='pre-line';}
function renderDemo(changeSource=true){
 const demo=demos[activeDemo],copy=demo,video=$('demo-video');
 document.querySelectorAll('[data-demo]').forEach(button=>{const active=button.dataset.demo===activeDemo;button.setAttribute('aria-selected',String(active));button.tabIndex=active?0:-1;if(active)$('demo-panel').setAttribute('aria-labelledby',button.id);});
 $('video-title').textContent=demo.label;$('demo-number').textContent=demo.number;lineText($('demo-name'),copy.name);$('demo-description').textContent=copy.description;$('demo-transcript').textContent=copy.transcript;$('demo-command').textContent=demo.command;video.setAttribute('aria-label',copy.name.replace('\n',' '));
 $('download-video').href=`assets/demos/${activeDemo}.mp4`;
 if(changeSource){video.pause();video.poster=`assets/demos/${activeDemo}.png`;video.querySelector('source').src=`assets/demos/${activeDemo}.mp4`;video.load();video.play().catch(()=>{});}
 const track=$('demo-captions');track.src=`assets/demos/${activeDemo}.vtt`;track.srclang='en';track.label='English';
}
const tabButtons=[...document.querySelectorAll('[data-demo]')];
tabButtons.forEach((button,index)=>{
 button.addEventListener('click',()=>{activeDemo=button.dataset.demo;renderDemo();});
 button.addEventListener('keydown',event=>{let next=index;if(event.key==='ArrowRight')next=(index+1)%tabButtons.length;else if(event.key==='ArrowLeft')next=(index+tabButtons.length-1)%tabButtons.length;else if(event.key==='Home')next=0;else if(event.key==='End')next=tabButtons.length-1;else return;event.preventDefault();tabButtons[next].focus();tabButtons[next].click();});
});
const targets={'aarch64-apple-darwin':'macOS · Apple Silicon','x86_64-apple-darwin':'macOS · Intel','x86_64-unknown-linux-musl':'Linux · x86_64','aarch64-unknown-linux-musl':'Linux · ARM64'};
function renderInstall(){
 const version=release?.tag_name||'vVERSION',filename=`flik-${version}-${selectedTarget}.tar.gz`;
 document.querySelectorAll('[data-target]').forEach(button=>button.setAttribute('aria-pressed',String(button.dataset.target===selectedTarget)));
 $('download-detail').textContent=`${targets[selectedTarget]} · .tar.gz`;
 $('install-command').textContent=`tar -xzf ${filename}\n./flik demo.log`;
 const github=$('github-link');github.hidden=!repository;if(repository)github.href=`https://github.com/${repository}`;
 const download=$('download-binary');const asset=release?.assets?.find(asset=>asset.name===filename);
 const validAsset=asset&&asset.browser_download_url===`https://github.com/${repository}/releases/download/${version}/${filename}`;
 download.setAttribute('aria-disabled',String(!validAsset));
 if(validAsset){download.href=asset.browser_download_url;download.querySelector('span').textContent='Download Flik';download.setAttribute('download','');}
 else{download.removeAttribute('href');download.querySelector('span').textContent='Binary available with the first release';}
 const checksums=release?.assets?.find(asset=>asset.name==='SHA256SUMS');
 const checksumURL=`https://github.com/${repository}/releases/download/${version}/SHA256SUMS`;
 $('checksum-link').hidden=!(checksums&&checksums.browser_download_url===checksumURL);
 if(!$('checksum-link').hidden)$('checksum-link').href=checksumURL;
 $('release-version').textContent=release?.tag_name||'RELEASES';
 $('release-status').textContent=release?`Release ${version} · macOS & Linux`:repository?'No public release detected yet':'First public release being prepared';
 $('installer-box').hidden=!release;$('installer-note').hidden=!!release;
 if(release){$('installer-command').textContent=`curl -fsSL https://raw.githubusercontent.com/${repository}/${version}/scripts/install.sh -o install-flik.sh\nFLIK_REPO=${repository} sh install-flik.sh`;}
}
document.querySelectorAll('[data-target]').forEach(button=>button.addEventListener('click',()=>{selectedTarget=button.dataset.target;renderInstall();}));
let toastTimer;
function toast(message){clearTimeout(toastTimer);$('toast').textContent=message;$('toast').hidden=false;toastTimer=setTimeout(()=>$('toast').hidden=true,3500);}
document.querySelectorAll('[data-copy]').forEach(button=>button.addEventListener('click',async()=>{
 const text=$(button.dataset.copy).textContent;
 try{if(!navigator.clipboard)throw new Error('clipboard unavailable');await navigator.clipboard.writeText(text);toast('Commands copied.');}
 catch{const selection=window.getSelection(),range=document.createRange();range.selectNodeContents($(button.dataset.copy));selection.removeAllRanges();selection.addRange(range);toast('Text selected. Copy it using your keyboard.');}
}));
$('download-binary').addEventListener('click',event=>{if(event.currentTarget.getAttribute('aria-disabled')==='true'){event.preventDefault();toast('The first public release is being prepared.');}});
async function getJSON(url){const controller=new AbortController();const timeout=setTimeout(()=>controller.abort(),8000);try{const response=await fetch(url,{signal:controller.signal,headers:{Accept:'application/json'}});if(!response.ok)throw new Error(`HTTP ${response.status}`);return await response.json();}finally{clearTimeout(timeout);}}
async function loadRelease(){
 try{const config=await getJSON('site-config.json');
 if(typeof config.repository!=='string'||!/^[-\w.]+\/[-\w.]+$/.test(config.repository))return;
 repository=config.repository;renderInstall();
 const suffix=config.releaseTag&&/^v\d+\.\d+\.\d+$/.test(config.releaseTag)?`tags/${config.releaseTag}`:'latest';
 const data=await getJSON(`https://api.github.com/repos/${repository}/releases/${suffix}`);
 if(!/^v\d+\.\d+\.\d+$/.test(data.tag_name)||data.draft||data.prerelease||!Array.isArray(data.assets))return;
 release=data;renderInstall();
 }catch{if(repository){$('release-status').textContent='Check GitHub for available releases.';}}
}
renderDemo(false);renderInstall();loadRelease();

// Demonstrate the real workflow immediately; respect explicit pauses and reduced motion.
const heroVideo=$('hero-video'),heroPlayback=$('hero-playback');
let heroManuallyPaused=false;
function syncHeroPlayback(){heroPlayback.textContent=heroVideo.paused?'Play walkthrough':'Pause walkthrough';heroPlayback.setAttribute('aria-label',heroVideo.paused?'Play the Kubernetes walkthrough':'Pause the Kubernetes walkthrough');}
heroVideo.addEventListener('play',syncHeroPlayback);heroVideo.addEventListener('pause',syncHeroPlayback);
heroPlayback.addEventListener('click',()=>{if(heroVideo.paused){heroManuallyPaused=false;heroVideo.play().catch(()=>{heroVideo.controls=true;});}else{heroManuallyPaused=true;heroVideo.pause();}});
$('hero-fullscreen').addEventListener('click',async()=>{heroVideo.controls=true;try{if(heroVideo.requestFullscreen)await heroVideo.requestFullscreen();else if(heroVideo.webkitEnterFullscreen)heroVideo.webkitEnterFullscreen();}catch{}heroManuallyPaused=false;heroVideo.play().catch(()=>{});});
document.addEventListener('fullscreenchange',()=>{if(!document.fullscreenElement)heroVideo.controls=false;});
if(!matchMedia('(prefers-reduced-motion: reduce)').matches&&'IntersectionObserver' in window){new IntersectionObserver(entries=>{for(const entry of entries){if(entry.isIntersecting&&!heroManuallyPaused)heroVideo.play().catch(()=>{});else heroVideo.pause();}},{threshold:.15}).observe(heroVideo);}
document.addEventListener('visibilitychange',()=>{if(document.hidden)heroVideo.pause();});
document.querySelector('.hero-actions a[href="#demos"]').addEventListener('click',()=>{heroVideo.pause();$('demo-video').play().catch(()=>{});});

const heroSteps=[...document.querySelectorAll('[data-hero-seek]')];
heroSteps.forEach(button=>button.addEventListener('click',()=>{heroVideo.currentTime=Number(button.dataset.heroSeek);heroManuallyPaused=false;heroVideo.play().catch(()=>{});}));
heroVideo.addEventListener('timeupdate',()=>{const current=heroVideo.currentTime;heroSteps.forEach((button,index)=>{const start=Number(button.dataset.heroSeek),next=heroSteps[index+1]?Number(heroSteps[index+1].dataset.heroSeek):Infinity;button.setAttribute('aria-pressed',String(current>=start&&current<next));});});
