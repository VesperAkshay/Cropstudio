import { invoke } from '@tauri-apps/api/core';
import { convertFileSrc } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';
import { getCurrentWebview } from '@tauri-apps/api/webview';

const $ = (id) => document.getElementById(id);
const els = {
  queueList:$('queueList'), queueCount:$('queueCount'), readyCount:$('readyCount'), doneCount:$('doneCount'), clearQueueBtn:$('clearQueueBtn'),
  addFilesBtn:$('addFilesBtn'), addFilesTop:$('addFilesTop'), emptyAddBtn:$('emptyAddBtn'), fileInput:$('fileInput'),
  stage:$('stage'), emptyState:$('emptyState'), mediaCanvas:$('mediaCanvas'), imagePreview:$('imagePreview'), videoPreview:$('videoPreview'), cropOverlay:$('cropOverlay'), cropBox:$('cropBox'), stageIndex:$('stageIndex'), stageDropHint:$('stageDropHint'),
  previousMediaBtn:$('previousMediaBtn'), nextMediaBtn:$('nextMediaBtn'), mediaPosition:$('mediaPosition'), activeFilename:$('activeFilename'),
  adjustCropBtn:$('adjustCropBtn'), drawCropBtn:$('drawCropBtn'), cropHelp:$('cropHelp'), fullFrameBtn:$('fullFrameBtn'), centerSquareBtn:$('centerSquareBtn'),
  aspectGrid:$('aspectGrid'), cropX:$('cropX'), cropY:$('cropY'), cropW:$('cropW'), cropH:$('cropH'),
  timelineCard:$('timelineCard'), trimStart:$('trimStart'), trimEnd:$('trimEnd'), trimStartRange:$('trimStartRange'), durationLabel:$('durationLabel'), resetTrimBtn:$('resetTrimBtn'),
  outputFormat:$('outputFormat'), processBtn:$('processBtn'), processProgress:$('processProgress'), processStage:$('processStage'), processPercent:$('processPercent'), processFill:$('processFill'),
  resultCard:$('resultCard'), resultName:$('resultName'), resultMeta:$('resultMeta'), saveResultBtn:$('saveResultBtn'), processBatchBtn:$('processBatchBtn'), batchCount:$('batchCount'), batchProgress:$('batchProgress'), batchStage:$('batchStage'), batchPercent:$('batchPercent'), batchFill:$('batchFill'), downloadBatchBtn:$('downloadBatchBtn'),
  statusLine:$('statusLine'), engineStatus:$('engineStatus'), autoCropBtn:$('autoCropBtn'), autoCropOptions:$('autoCropOptions'), autoMode:$('autoMode'), windowSeconds:$('windowSeconds'), runAutoBtn:$('runAutoBtn'), autoSummary:$('autoSummary'), toast:$('toast'), settingsBtn:$('settingsBtn'),
  queueSearch:$('queueSearch'), queueFilter:$('queueFilter'), activeMeta:$('activeMeta'), copyCropBtn:$('copyCropBtn'), applyCropAllBtn:$('applyCropAllBtn'),
  filmstrip:$('filmstrip'), trimEndRange:$('trimEndRange'), compareBtn:$('compareBtn'), lutSelect:$('lutSelect'),
  brightness:$('brightness'), contrast:$('contrast'), saturation:$('saturation'), sharpness:$('sharpness'),
  brightnessValue:$('brightnessValue'), contrastValue:$('contrastValue'), saturationValue:$('saturationValue'), sharpnessValue:$('sharpnessValue'),
  watermarkText:$('watermarkText'), exportResolution:$('exportResolution'), exportBitrate:$('exportBitrate'), exportFps:$('exportFps'), exportEncoder:$('exportEncoder'),
  resetLookBtn:$('resetLookBtn'), exportEstimate:$('exportEstimate'), lutFileBtn:$('lutFileBtn'), clearLutBtn:$('clearLutBtn'), lutFileLabel:$('lutFileLabel')
};
const queue = [];
let activeId = null;
let cropMode = 'adjust';
let gesture = null;
let batchRunning = false;
let batchContext = null;
let toastTimer = null;
let cropClipboard = null;
const STORE_KEY = 'cropstudio.rust.queue.v2';
const defaultFormats = {image:'png',video:'mp4',audio:'mp3'};
const supported = ['image','video','audio'];
const clamp=(v,min,max)=>Math.max(min,Math.min(max,v));
const num=(v,f=0)=>Number.isFinite(Number(v))?Number(v):f;
const fmtTime=(v)=>{const n=Math.max(0,num(v));return `${Math.floor(n/60).toString().padStart(2,'0')}:${Math.floor(n%60).toString().padStart(2,'0')}`;};
const formatMeta=(item)=>{if(!item)return 'IMPORT MEDIA TO BEGIN';const m=item.metadata||{};const dims=m.width&&m.height?`${m.width} × ${m.height}`:item.mediaType.toUpperCase();const duration=m.duration?` · ${fmtTime(m.duration)}`:'';const fps=m.frame_rate?` · ${Number(m.frame_rate).toFixed(2)} FPS`:'';return `${dims}${duration}${fps} · ${(m.codec||item.mediaType).toUpperCase()}`;};
const ratioOf=(v)=>{if(!v||v==='free')return null;const [a,b]=v.split(':').map(Number);return b? a/b:null;};
const active=()=>queue.find(x=>x.mediaId===activeId)||null;
const mediaPath=(item)=>convertFileSrc(item.path);
function toast(message){els.toast.textContent=message;els.toast.classList.remove('hidden');clearTimeout(toastTimer);toastTimer=setTimeout(()=>els.toast.classList.add('hidden'),2600);}
function status(message,kind='info'){const dot=document.createElement('i');dot.className='status-dot';els.statusLine.replaceChildren(dot,document.createTextNode(String(message)));els.statusLine.dataset.kind=kind;}
function persist(){try{localStorage.setItem(STORE_KEY,JSON.stringify({activeId,items:queue.map(({mediaId,filename,path,mediaType,metadata,crop,aspect,trimStart,trimEnd,outputFormat,adaptivePlan,result,status,error,adjustments,lut,lutPath,watermark,exportSettings})=>({mediaId,filename,path,mediaType,metadata,crop,aspect,trimStart,trimEnd,outputFormat,adaptivePlan,result,status,error,adjustments,lut,lutPath,watermark,exportSettings}))}));}catch{}}
function dimensions(item){return {w:num(item?.metadata?.width),h:num(item?.metadata?.height)};}
function fullCrop(item){const {w,h}=dimensions(item);return {x:0,y:0,width:w,height:h};}
function normalizeCrop(c,item=itemActiveFallback()){if(!item)return c;const {w,h}=dimensions(item);const width=clamp(Math.round(c.width),1,w||1),height=clamp(Math.round(c.height),1,h||1);return {x:clamp(Math.round(c.x),0,Math.max(0,w-width)),y:clamp(Math.round(c.y),0,Math.max(0,h-height)),width,height};}
function itemActiveFallback(){return active();}
function fittedCrop(c,ratio,item){const {w,h}=dimensions(item);let out=normalizeCrop(c,item);if(!ratio)return out;let cw=out.width,ch=out.height;if(cw/ch>ratio)cw=Math.floor(ch*ratio);else ch=Math.floor(cw/ratio);cw=clamp(cw,1,w);ch=clamp(ch,1,h);const cx=out.x+out.width/2,cy=out.y+out.height/2;return normalizeCrop({x:Math.round(cx-cw/2),y:Math.round(cy-ch/2),width:cw,height:ch},item);}
function markDirty(item){if(!item?.result)return;item.result=null;item.status='ready';item.error=null;if(item.mediaId===activeId)els.resultCard.classList.add('hidden');renderQueue();}
function saveActive(){const item=active();if(!item)return;item.crop={x:num(els.cropX.value),y:num(els.cropY.value),width:num(els.cropW.value),height:num(els.cropH.value)};item.trimStart=num(els.trimStart.value);item.trimEnd=num(els.trimEnd.value);item.outputFormat=els.outputFormat.value;item.adjustments={brightness:num(els.brightness.value),contrast:num(els.contrast.value)/100,saturation:num(els.saturation.value)/100,sharpness:num(els.sharpness.value)};item.lut=els.lutSelect.value;item.watermark=els.watermarkText.value;item.exportSettings={resolution:els.exportResolution.value,bitrate:els.exportBitrate.value,frameRate:els.exportFps.value,encoder:els.exportEncoder.value};persist();}
function setCrop(crop,{updateAspect=false}={}){const item=active();if(!item)return;markDirty(item);item.adaptivePlan=null;item.crop=normalizeCrop(crop,item);if(updateAspect){item.aspect='free';document.querySelectorAll('.aspect-btn').forEach(b=>b.classList.toggle('active',b.dataset.aspect==='free'));}renderCrop();persist();}
function renderCrop(){const item=active();if(!item)return;const c=normalizeCrop(item.crop||fullCrop(item),item);item.crop=c;const {w,h}=dimensions(item);if(!w||!h)return;els.cropBox.style.left=`${c.x/w*100}%`;els.cropBox.style.top=`${c.y/h*100}%`;els.cropBox.style.width=`${c.width/w*100}%`;els.cropBox.style.height=`${c.height/h*100}%`;els.cropX.value=c.x;els.cropY.value=c.y;els.cropW.value=c.width;els.cropH.value=c.height;}
function fitCanvas(item){if(!item)return;const {w,h}=dimensions(item);if(!w||!h)return;const stageRect=els.stage.getBoundingClientRect();const maxW=Math.max(100,stageRect.width-42),maxH=Math.max(100,stageRect.height-42);const scale=Math.min(maxW/w,maxH/h);const cw=Math.max(1,Math.round(w*scale)),ch=Math.max(1,Math.round(h*scale));els.mediaCanvas.style.width=`${cw}px`;els.mediaCanvas.style.height=`${ch}px`;renderCrop();}
function renderTimeline(item){
  const video=item?.mediaType==='video';els.timelineCard.classList.toggle('disabled',!video);
  els.trimStart.disabled=!video;els.trimEnd.disabled=!video;els.trimStartRange.disabled=!video;els.trimEndRange.disabled=!video;
  const duration=num(item?.metadata?.duration);els.durationLabel.textContent=fmtTime(duration);
  els.trimStart.max=duration;els.trimEnd.max=duration;els.trimStartRange.max=duration;els.trimEndRange.max=duration;
  els.trimStart.value=num(item?.trimStart).toFixed(2);els.trimEnd.value=num(item?.trimEnd,duration).toFixed(2);
  els.trimStartRange.value=num(item?.trimStart);els.trimEndRange.value=num(item?.trimEnd,duration);
  renderFilmstrip(item);
}
function renderFormat(item){const options=item?.mediaType==='image'?[['png','PNG'],['jpg','JPEG'],['webp','WEBP']]:item?.mediaType==='audio'?[['mp3','MP3'],['wav','WAV'],['m4a','M4A'],['flac','FLAC']]:[['mp4','MP4 · H.264'],['webm','WEBM · VP9']];els.outputFormat.innerHTML=options.map(([v,l])=>`<option value="${v}">${l}</option>`).join('');els.outputFormat.value=item?.outputFormat||defaultFormats[item?.mediaType]||'mp4';const video=item?.mediaType==='video';els.exportBitrate.disabled=!video;els.exportFps.disabled=!video;els.exportEncoder.disabled=!video||els.outputFormat.value==='webm';els.exportResolution.disabled=item?.mediaType==='audio';updateExportEstimate(item);}
function renderActive(){const item=active();const has=!!item;if(!has||item.mediaType!=='video'||els.videoPreview.dataset.path!==item.path)els.videoPreview.pause();els.emptyState.classList.toggle('hidden',has);els.mediaCanvas.classList.toggle('hidden',!has||item.mediaType==='audio');els.imagePreview.classList.toggle('hidden',!has||item.mediaType!=='image');els.videoPreview.classList.toggle('hidden',!has||item.mediaType!=='video');els.cropOverlay.classList.toggle('hidden',!has||item.mediaType==='audio');els.activeFilename.textContent=has?item.filename.toUpperCase():'NO MEDIA SELECTED';els.activeMeta.textContent=formatMeta(item);els.stageIndex.textContent=has?String(queue.findIndex(x=>x.mediaId===activeId)+1).padStart(2,'0'):'—';els.previousMediaBtn.disabled=!has||queue.length<2;els.nextMediaBtn.disabled=!has||queue.length<2;const ix=has?queue.findIndex(x=>x.mediaId===activeId)+1:0;els.mediaPosition.textContent=`${ix} / ${queue.length}`;els.processBtn.disabled=!has||item.status==='processing';els.processBtn.textContent=item?.status==='processing'?'EXPORTING…':'EXPORT THIS FILE ↗';els.resultCard.classList.toggle('hidden',!item?.result);els.compareBtn.disabled=!item?.result||item?.mediaType==='audio';if(item?.result){els.resultName.textContent=item.result.filename;els.resultMeta.textContent=`${item.result.metadata?.width||'—'} × ${item.result.metadata?.height||'—'} · ready to save`;}else{els.processProgress.classList.add('hidden');}
  if(has){if(item.mediaType==='image'){els.imagePreview.src=mediaPath(item);els.imagePreview.onload=()=>fitCanvas(item);}else if(item.mediaType==='video'){const src=mediaPath(item);if(els.videoPreview.dataset.path!==item.path){els.videoPreview.pause();els.videoPreview.src=src;els.videoPreview.poster=item.thumbnailPath?convertFileSrc(item.thumbnailPath):'';els.videoPreview.dataset.path=item.path;els.videoPreview.onloadedmetadata=()=>fitCanvas(item);els.videoPreview.load();}else{els.videoPreview.poster=item.thumbnailPath?convertFileSrc(item.thumbnailPath):'';fitCanvas(item);}}else{els.imagePreview.classList.add('hidden');els.videoPreview.classList.add('hidden');els.mediaCanvas.classList.add('hidden');els.stage.querySelector('.empty-state').classList.remove('hidden');els.stage.querySelector('.empty-headline').innerHTML='AUDIO<br><em>ONLY.</em>';els.stage.querySelector('.empty-state p').textContent='Audio trimming is supported by the engine; image/video preview is the main workspace.';}}
  if(has){renderCrop();renderTimeline(item);renderFormat(item);renderAdjustmentControls(item);document.querySelectorAll('.aspect-btn').forEach(b=>b.classList.toggle('active',b.dataset.aspect===(item.aspect||'free')));els.autoCropBtn.disabled=item.mediaType==='audio';}else{renderTimeline(null);els.outputFormat.innerHTML='<option>MP4</option>';els.cropX.value=els.cropY.value=els.cropW.value=els.cropH.value='';els.emptyState.querySelector('.empty-headline').innerHTML='MAKE<br><em>THE FRAME.</em>';els.emptyState.querySelector('p').textContent='Drag media here or use the button below.';}
  renderQueue();}
function updateExportEstimate(item=active()){
  if(!item||item.mediaType!=='video'){els.exportEstimate.textContent='Size estimate is available for video exports.';return;}
  const rate=els.exportBitrate.value;
  const duration=Math.max(.05,num(item.trimEnd,num(item.metadata.duration))-num(item.trimStart));
  if(rate==='auto'){els.exportEstimate.textContent=`Approx. duration: ${fmtTime(duration)} · Auto quality (CRF/CQ)`;return;}
  const mb=duration*Number.parseFloat(rate)*1000000/8/1000000;
  els.exportEstimate.textContent=`Estimated video payload: ~${mb.toFixed(1)} MB · excludes audio and container overhead`;
}
async function chooseLut(){
  const item=active();if(!item)return;
  try{
    const path=await open({multiple:false,directory:false,title:'Choose a 3D LUT (.cube)',filters:[{name:'3D LUT',extensions:['cube']}]});
    if(!path)return;
    markDirty(item);item.lutPath=Array.isArray(path)?path[0]:path;
    els.lutFileLabel.textContent=`CUSTOM LUT · ${String(item.lutPath).split(/[\\/]/).pop()}`;
    saveActive();toast('Custom .cube LUT loaded.');
  }catch(e){toast(`Could not load LUT: ${e}`);}
}
function clearLut(){
  const item=active();if(!item)return;markDirty(item);item.lutPath=null;
  els.lutFileLabel.textContent='No custom LUT loaded.';saveActive();toast('Custom LUT cleared.');
}
function resetLook(){
  const item=active();if(!item)return;markDirty(item);
  els.brightness.value=0;els.contrast.value=100;els.saturation.value=100;els.sharpness.value=0;els.lutSelect.value='none';els.watermarkText.value='';item.lutPath=null;els.lutFileLabel.textContent='No custom LUT loaded.';
  updateAdjustmentControl();saveActive();toast('Look reset to original.');
}
function renderAdjustmentControls(item){
  const a=item.adjustments||{brightness:0,contrast:1,saturation:1,sharpness:0};
  els.brightness.value=a.brightness??0;els.contrast.value=Math.round((a.contrast??1)*100);els.saturation.value=Math.round((a.saturation??1)*100);els.sharpness.value=a.sharpness??0;
  els.brightnessValue.textContent=String(els.brightness.value);els.contrastValue.textContent=`${els.contrast.value}%`;els.saturationValue.textContent=`${els.saturation.value}%`;els.sharpnessValue.textContent=String(els.sharpness.value);
  els.lutSelect.value=item.lut||'none';els.watermarkText.value=item.watermark||'';const audio=item.mediaType==='audio';[els.brightness,els.contrast,els.saturation,els.sharpness,els.lutSelect,els.watermarkText,els.lutFileBtn,els.clearLutBtn,els.resetLookBtn].forEach(control=>control.disabled=audio);els.lutFileLabel.textContent=item.lutPath?`CUSTOM LUT · ${item.lutPath.split(/[\\/]/).pop()}`:'No custom LUT loaded.';
  const e=item.exportSettings||{};
  els.exportResolution.value=e.resolution||'source';els.exportBitrate.value=e.bitrate||'auto';els.exportFps.value=e.frameRate||'source';els.exportEncoder.value=e.encoder||'auto';updateExportEstimate(item);
}
function renderFilmstrip(item){
  els.filmstrip.replaceChildren();
  if(!item||item.mediaType!=='video'||!item.filmstrip?.length){
    const hint=document.createElement('span');hint.className='filmstrip-empty';hint.textContent=item?.mediaType==='video'?'No filmstrip frames available — check FFmpeg.':'VIDEO FILMSTRIP PREVIEWS';els.filmstrip.appendChild(hint);return;
  }
  item.filmstrip.forEach((path,index)=>{
    const button=document.createElement('button');button.type='button';button.className='filmstrip-frame';button.title=`Seek to ${fmtTime(num(item.metadata.duration)*index/Math.max(1,item.filmstrip.length-1))}`;
    const img=document.createElement('img');img.src=convertFileSrc(path);img.alt=`Video frame ${index+1}`;img.loading='lazy';button.appendChild(img);
    button.addEventListener('click',()=>{const duration=num(item.metadata.duration);const t=duration*(index/Math.max(1,item.filmstrip.length-1));els.videoPreview.currentTime=t;status(`Preview at ${fmtTime(t)}.`);});
    els.filmstrip.appendChild(button);
  });
}
function renderQueue(){
  els.queueList.innerHTML='';
  const query=(els.queueSearch?.value||'').trim().toLowerCase();
  const filter=els.queueFilter?.value||'all';
  const visible=queue.filter(item=>(filter==='all'||item.mediaType===filter)&&(!query||item.filename.toLowerCase().includes(query)));
  if(!queue.length){
    els.queueList.innerHTML='<div class="queue-empty"><span class="empty-icon">↘</span><strong>Nothing in the queue.</strong><span>Add images or videos to start cropping.</span></div>';
  }else if(!visible.length){
    els.queueList.innerHTML='<div class="queue-empty queue-no-results"><span class="empty-icon">⌕</span><strong>No matching media.</strong><span>Try another name or media filter.</span></div>';
  }
  visible.forEach((item,index)=>{
    const row=document.createElement('button');
    row.type='button';
    row.className=`queue-item ${item.mediaId===activeId?'active':''}`;
    row.style.setProperty('--item-index',Math.min(index,12));
    row.innerHTML=`<span class="queue-thumb ${item.mediaType}-thumb">${item.mediaType==='video'?'▶':item.mediaType==='image'?'▧':'♫'}</span><span class="queue-file"><strong title="${escapeHtml(item.filename)}">${escapeHtml(item.filename)}</strong><span>${item.mediaType} · ${item.metadata?.width||'—'}×${item.metadata?.height||'—'}</span></span><span class="queue-state ${item.status==='completed'?'done':item.status==='error'?'error':''}">${item.status==='completed'?'✓':item.status==='processing'?'…':item.status==='error'?'!':'—'}</span>`;
    const thumb=row.querySelector('.queue-thumb');
    if(item.thumbnailPath){
      const image=document.createElement('img');
      image.src=convertFileSrc(item.thumbnailPath);
      image.alt=`Preview of ${item.filename}`;
      image.loading='lazy';
      image.decoding='async';
      thumb.replaceChildren(image);
    }
    row.addEventListener('click',()=>selectItem(item.mediaId));
    els.queueList.appendChild(row);
  });
  const done=queue.filter(x=>x.status==='completed').length;
  const ready=queue.filter(x=>x.status!=='completed').length;
  els.queueCount.textContent=String(queue.length).padStart(2,'0');
  els.readyCount.textContent=ready;
  els.doneCount.textContent=done;
  els.clearQueueBtn.disabled=!queue.length||batchRunning;
  els.processBatchBtn.disabled=!queue.some(x=>x.status!=='completed'&&x.status!=='processing')||batchRunning;
  els.batchCount.textContent=`${queue.length} FILES`;
  els.downloadBatchBtn.classList.toggle('hidden',done===0);
  persist();
}
function escapeHtml(value){return String(value).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
function selectItem(id){if(compareSnapshot)endCompare();saveActive();activeId=id;const item=active();if(item?.result){els.resultCard.classList.remove('hidden');}else{els.resultCard.classList.add('hidden');}renderActive();status(`Loaded ${item?.filename||'media'}.`);}
function moveSelection(delta){if(!queue.length)return;saveActive();let i=queue.findIndex(x=>x.mediaId===activeId);for(let step=0;step<queue.length;step++){i=(i+delta+queue.length)%queue.length;if(queue[i].mediaType!=='audio'){selectItem(queue[i].mediaId);return;}}}
async function chooseFiles(){try{const paths=await open({multiple:true,directory:false,title:'Add images and videos',filters:[{name:'Media',extensions:['jpg','jpeg','png','webp','bmp','tif','tiff','gif','mp4','mov','m4v','mkv','webm','avi','mpeg','mpg','mp3','wav','m4a','flac','aac','ogg']} ]});if(!paths)return;await addPaths(Array.isArray(paths)?paths:[paths]);}catch(e){toast(`Could not open files: ${e}`);}}
async function addPaths(paths){const unique=paths.filter(p=>!queue.some(x=>x.path===p));if(!unique.length){toast('Those files are already in the queue.');return;}status(`Importing ${unique.length} file(s)…`);try{const imported=await invoke('import_media',{paths:unique});for(const m of imported){if(!supported.includes(m.mediaType))continue;const duration=num(m.metadata?.duration);queue.push({...m,crop:fullCrop(m),aspect:'free',trimStart:0,trimEnd:duration,outputFormat:defaultFormats[m.mediaType],adjustments:{brightness:0,contrast:1,saturation:1,sharpness:0},lut:'none',lutPath:null,watermark:'',exportSettings:{resolution:'source',bitrate:'auto',frameRate:'source',encoder:'auto'},status:'ready',result:null,error:null});}if(!activeId&&queue.length)activeId=queue[0].mediaId;renderActive();status(`Added ${imported.length} file(s) to the queue.`,'success');toast(`Added ${imported.length} media file(s)`);}catch(e){status(String(e),'error');toast(String(e));}}
async function restoreQueue(){try{const saved=JSON.parse(localStorage.getItem(STORE_KEY)||localStorage.getItem('cropstudio.rust.queue.v1')||'{}');if(!saved.items?.length)return;const paths=saved.items.map(x=>x.path);const restored=await invoke('restore_media',{paths});for(const m of restored){const old=saved.items.find(x=>x.path===m.path)||{};queue.push({...m,crop:old.crop||fullCrop(m),aspect:old.aspect||'free',trimStart:old.trimStart||0,trimEnd:old.trimEnd??num(m.metadata?.duration),outputFormat:old.outputFormat||defaultFormats[m.mediaType],adaptivePlan:old.adaptivePlan||null,adjustments:old.adjustments||{brightness:0,contrast:1,saturation:1,sharpness:0},lut:old.lut||'none',lutPath:old.lutPath||null,watermark:old.watermark||'',exportSettings:old.exportSettings||{resolution:'source',bitrate:'auto',frameRate:'source',encoder:'auto'},status:old.result?'completed':'ready',result:old.result||null,error:null});}activeId=queue.some(x=>x.mediaId===saved.activeId)?saved.activeId:queue[0]?.mediaId||null;renderActive();}catch(e){console.warn('Could not restore previous queue',e);}}
function cropRatio(item){const ratio=ratioOf(item.aspect);if(!ratio)return null;const {w,h}=dimensions(item);return ratio;}
function updateAspect(aspect){const item=active();if(!item)return;saveActive();markDirty(item);item.adaptivePlan=null;item.aspect=aspect;const c=item.crop||fullCrop(item);item.crop=fittedCrop(c,cropRatio(item),item);renderCrop();document.querySelectorAll('.aspect-btn').forEach(b=>b.classList.toggle('active',b.dataset.aspect===aspect));persist();}
function pointerPoint(e){const r=els.cropOverlay.getBoundingClientRect();return {x:clamp((e.clientX-r.left)/r.width,0,1),y:clamp((e.clientY-r.top)/r.height,0,1)};}
function cropToNorm(c,item){const {w,h}=dimensions(item);return {x:c.x/w,y:c.y/h,w:c.width/w,h:c.height/h};}
function normToCrop(n,item){const {w,h}=dimensions(item);return normalizeCrop({x:n.x*w,y:n.y*h,width:n.w*w,height:n.h*h},item);}
function startGesture(e,kind,handle=''){const item=active();if(!item||item.mediaType==='audio')return;e.preventDefault();e.stopPropagation();saveActive();const p=pointerPoint(e);const c=cropToNorm(item.crop||fullCrop(item),item);gesture={kind,handle,start:p,rect:c,ratio:cropRatio(item),itemId:item.mediaId};els.cropBox.setPointerCapture?.(e.pointerId);}
function resizeGesture(g,p){const dx=p.x-g.start.x,dy=p.y-g.start.y;let r={...g.rect};const h=g.handle;const left=h.includes('w'),right=h.includes('e'),top=h.includes('n'),bottom=h.includes('s');const min=.012;if(g.kind==='draw'){r={x:Math.min(g.start.x,p.x),y:Math.min(g.start.y,p.y),w:Math.max(min,Math.abs(dx)),h:Math.max(min,Math.abs(dy))};if(g.ratio){const item=active();const {w,h}=dimensions(item);const dr=g.ratio*h/w;if(r.w/r.h>dr)r.w=r.h*dr;else r.h=r.w/dr;if(p.x<g.start.x)r.x=g.start.x-r.w;if(p.y<g.start.y)r.y=g.start.y-r.h;r.x=clamp(r.x,0,1-r.w);r.y=clamp(r.y,0,1-r.h);}}else if(g.kind==='move'){r.x=clamp(g.rect.x+dx,0,1-r.w);r.y=clamp(g.rect.y+dy,0,1-r.h);}else{let x1=g.rect.x,x2=g.rect.x+g.rect.w,y1=g.rect.y,y2=g.rect.y+g.rect.h;if(left)x1=clamp(g.rect.x+dx,0,x2-min);if(right)x2=clamp(g.rect.x+g.rect.w+dx,x1+min,1);if(top)y1=clamp(g.rect.y+dy,0,y2-min);if(bottom)y2=clamp(g.rect.y+g.rect.h+dy,y1+min,1);if(g.ratio){const item=active();const {w,h}=dimensions(item);const displayRatio=g.ratio*h/w;let nw=x2-x1,nh=y2-y1;if(left||right){nh=nw/displayRatio;if(top&&!bottom)y1=g.rect.y+g.rect.h-nh;else if(!top&&!bottom){const cy=g.rect.y+g.rect.h/2;y1=cy-nh/2;}y2=y1+nh;}else{nw=nh*displayRatio;if(left&&!right)x1=g.rect.x+g.rect.w-nw;else{const cx=g.rect.x+g.rect.w/2;x1=cx-nw/2;}x2=x1+nw;}const scale=Math.min(1/(x2-x1),1/(y2-y1),1);if(scale<1){nw*=scale;nh*=scale;if(left&&!right)x1=g.rect.x+g.rect.w-nw;else x2=x1+nw;if(top&&!bottom)y1=g.rect.y+g.rect.h-nh;else y2=y1+nh;}}r={x:x1,y:y1,w:x2-x1,h:y2-y1};r.x=clamp(r.x,0,1-r.w);r.y=clamp(r.y,0,1-r.h);}
  const item=active();if(!item||item.mediaId!==g.itemId)return;markDirty(item);item.adaptivePlan=null;item.crop=normToCrop(r,item);renderCrop();persist();}
function finishGesture(){if(!gesture)return;const was=gesture;gesture=null;if(was.kind==='draw'){const item=active();if(item){item.crop=normalizeCrop(item.crop,item);renderCrop();}}saveActive();}
function setupCropGestures(){els.cropOverlay.addEventListener('pointerdown',e=>{if(cropMode==='draw'){startGesture(e,'draw');return;}if(e.target.closest('.handle')){startGesture(e,'resize',e.target.closest('.handle').dataset.handle);return;}if(e.target===els.cropOverlay){const p=pointerPoint(e);const item=active();const c=cropToNorm(item.crop,item);if(p.x>=c.x&&p.x<=c.x+c.w&&p.y>=c.y&&p.y<=c.y+c.h)startGesture(e,'move');else startGesture(e,'draw');return;}if(e.target===els.cropBox)startGesture(e,'move');});window.addEventListener('pointermove',e=>{if(gesture)resizeGesture(gesture,pointerPoint(e));});window.addEventListener('pointerup',finishGesture);window.addEventListener('pointercancel',finishGesture);}
function setCropMode(mode){cropMode=mode;els.stage.classList.toggle('draw-mode',mode==='draw');els.adjustCropBtn.classList.toggle('active',mode==='adjust');els.drawCropBtn.classList.toggle('active',mode==='draw');els.adjustCropBtn.setAttribute('aria-pressed',String(mode==='adjust'));els.drawCropBtn.setAttribute('aria-pressed',String(mode==='draw'));els.cropHelp.textContent=mode==='draw'?'Drag anywhere on the media to draw a new crop':'Drag inside to move · pull handles to resize';}
function applySocialPreset(name){
  const map={'ig-post':'4:5','ig-story':'9:16','yt-thumb':'16:9','shorts':'9:16','fb-cover':'2.63:1','x-post':'16:9'};
  const ratio=map[name];if(!ratio)return;updateAspect(ratio);
  const labels={'ig-post':'Instagram portrait post','ig-story':'Instagram Story / Reel','yt-thumb':'YouTube thumbnail','shorts':'Shorts / TikTok','fb-cover':'Facebook cover','x-post':'X post'};
  toast(`${labels[name]} frame applied.`);
}
function updateAdjustmentControl(){
  const item=active();if(!item)return;markDirty(item);
  els.brightnessValue.textContent=String(els.brightness.value);
  els.contrastValue.textContent=`${els.contrast.value}%`;
  els.saturationValue.textContent=`${els.saturation.value}%`;
  els.sharpnessValue.textContent=String(els.sharpness.value);
  saveActive();
}
function setTrimStart(value){
  const item=active();if(!item)return;markDirty(item);
  item.trimStart=clamp(num(value),0,Math.max(0,num(item.metadata.duration)-.05));
  if(item.trimEnd<=item.trimStart)item.trimEnd=Math.min(num(item.metadata.duration),item.trimStart+.05);
  els.trimStart.value=item.trimStart.toFixed(2);els.trimEnd.value=item.trimEnd.toFixed(2);
  els.trimStartRange.value=item.trimStart;els.trimEndRange.value=item.trimEnd;persist();
}
function setTrimEnd(value){
  const item=active();if(!item)return;markDirty(item);
  item.trimEnd=clamp(num(value),(item.trimStart||0)+.05,num(item.metadata.duration));
  els.trimEnd.value=item.trimEnd.toFixed(2);els.trimEndRange.value=item.trimEnd;persist();
}
let compareSnapshot=null;
function beginCompare(){
  const item=active();if(!item?.result)return;
  compareSnapshot={itemId:item.mediaId,imageSrc:els.imagePreview.src,videoSrc:els.videoPreview.src,videoTime:els.videoPreview.currentTime,wasPlaying:!els.videoPreview.paused};
  const snap=compareSnapshot;
  els.cropOverlay.classList.add('hidden');els.imagePreview.style.objectFit='contain';els.videoPreview.style.objectFit='contain';
  if(item.mediaType==='image'){els.imagePreview.src=convertFileSrc(item.result.path);}
  else if(item.mediaType==='video'){els.videoPreview.pause();els.videoPreview.src=convertFileSrc(item.result.path);els.videoPreview.dataset.path='__compare__';els.videoPreview.onloadedmetadata=()=>{fitCanvas(item);els.videoPreview.currentTime=clamp(snap.videoTime-num(item.trimStart),0,Math.max(0,num(item.trimEnd)-num(item.trimStart)));};els.videoPreview.load();}
  els.compareBtn.classList.add('comparing');els.compareBtn.textContent='SHOWING CROPPED · RELEASE';
}
function endCompare(){
  if(!compareSnapshot)return;
  const item=active();const snap=compareSnapshot;compareSnapshot=null;
  if(item?.mediaId===snap.itemId){
    if(item.mediaType==='image')els.imagePreview.src=snap.imageSrc;
    else if(item.mediaType==='video'){els.videoPreview.pause();els.videoPreview.src=snap.videoSrc;els.videoPreview.dataset.path=item.path;els.videoPreview.load();els.videoPreview.addEventListener('loadedmetadata',()=>{els.videoPreview.currentTime=snap.videoTime;if(snap.wasPlaying)els.videoPreview.play().catch(()=>{});},{once:true});}
    els.cropOverlay.classList.remove('hidden');
  }
  els.imagePreview.style.objectFit='fill';els.videoPreview.style.objectFit='fill';
  els.compareBtn.classList.remove('comparing');els.compareBtn.textContent='HOLD TO COMPARE ◉';
}
function applyNumericCrop(){const item=active();if(!item)return;setCrop({x:num(els.cropX.value),y:num(els.cropY.value),width:num(els.cropW.value),height:num(els.cropH.value)});}
function setFullFrame(){const item=active();if(!item)return;setCrop(fullCrop(item));updateAspect('free');}
function setCenterSquare(){const item=active();if(!item)return;const {w,h}=dimensions(item);const side=Math.min(w,h);setCrop({x:(w-side)/2,y:(h-side)/2,width:side,height:side});updateAspect('1:1');}
function copyCropFrame(){
  const item=active();
  if(!item||item.mediaType==='audio'){toast('Select an image or video first.');return;}
  saveActive();
  cropClipboard={rect:cropToNorm(item.crop||fullCrop(item),item),aspect:item.aspect||'free'};
  toast('Crop frame copied. Apply it to the rest of the queue.');
}
function applyCropToAll(){
  if(!cropClipboard){toast('Copy a crop frame first.');return;}
  const source=active();
  let count=0;
  for(const item of queue){
    if(item.mediaType==='audio'||item.mediaId===source?.mediaId)continue;
    const {w,h}=dimensions(item);
    if(!w||!h)continue;
    const n=cropClipboard.rect;
    const raw={x:Math.round(n.x*w),y:Math.round(n.y*h),width:Math.round(n.w*w),height:Math.round(n.h*h)};
    item.aspect=cropClipboard.aspect;
    item.crop=fittedCrop(raw,ratioOf(item.aspect),item);
    item.adaptivePlan=null;
    markDirty(item);
    count++;
  }
  if(source)renderActive();
  renderQueue();
  toast(count?`Frame applied to ${count} other item(s).`:'No other images or videos in the queue.');
}
function payloadFor(item){return {mediaId:item.mediaId,trimStart:item.trimStart??0,trimEnd:item.trimEnd??item.metadata.duration,segments:[],crop:item.mediaType==='image'||item.mediaType==='video'?item.crop:null,adaptivePlan:item.adaptivePlan||null,outputFormat:item.outputFormat||defaultFormats[item.mediaType],adjustments:item.adjustments||{brightness:0,contrast:1,saturation:1,sharpness:0},lut:item.lut||'none',lutPath:item.lutPath||null,watermark:item.watermark||'',export:item.exportSettings||{resolution:'source',bitrate:'auto',frameRate:'source',encoder:'auto'}};}
async function runProcess(item,progressPrefix=''){item.status='processing';item.error=null;renderQueue();els.processProgress.classList.remove('hidden');els.processStage.textContent=`${progressPrefix}Preparing`;els.processPercent.textContent='0%';els.processFill.style.width='0%';try{const result=await invoke('process_media',{request:payloadFor(item)});item.result=result;item.status='completed';item.error=null;status(`Export complete: ${item.filename}`,'success');if(item.mediaId===activeId){els.resultCard.classList.remove('hidden');els.resultName.textContent=result.filename;els.resultMeta.textContent=`${result.metadata?.width||'—'} × ${result.metadata?.height||'—'} · ready to save`;els.processStage.textContent='Complete';els.processPercent.textContent='100%';els.processFill.style.width='100%';}persist();renderQueue();return result;}catch(e){item.status='error';item.error=String(e);status(`Export failed: ${e}`,'error');toast(`Export failed: ${e}`);throw e;}}
async function saveOutput(path,filename){const destination=await save({defaultPath:filename,filters:[{name:'Media',extensions:[filename.split('.').pop()||'mp4']}]});if(!destination)return;await invoke('copy_file',{source:path,destination});toast(`Saved ${filename}`);status(`Saved ${filename}`,'success');}
async function processActive(){const item=active();if(!item)return;saveActive();els.processBtn.disabled=true;try{await runProcess(item); }catch{}finally{els.processBtn.disabled=false;renderActive();}}
async function processBatch(){if(batchRunning)return;saveActive();const pending=queue.filter(x=>x.status!=='completed'&&x.status!=='processing');if(!pending.length){toast('Everything is already exported.');return;}batchRunning=true;batchContext={total:pending.length,completed:0};els.batchProgress.classList.remove('hidden');els.processBatchBtn.disabled=true;let finished=0,failed=0;for(let i=0;i<pending.length;i++){const item=pending[i];els.batchStage.textContent=`${i+1}/${pending.length} · ${item.filename}`;try{await runProcess(item,`${i+1}/${pending.length} · `);}catch{failed++;}finished++;batchContext.completed=finished;const pct=Math.round(finished/pending.length*100);els.batchPercent.textContent=`${pct}%`;els.batchFill.style.width=`${pct}%`;renderQueue();}batchRunning=false;batchContext=null;els.batchStage.textContent=failed?`Done · ${failed} failed`:`All ${pending.length} exports complete`;els.batchPercent.textContent='100%';els.batchFill.style.width='100%';status(failed?`Batch finished with ${failed} error(s).`:`Batch complete · ${pending.length} files exported.`,failed?'error':'success');renderQueue();}
async function saveBatch(){const outputs=queue.filter(x=>x.result?.path).map(x=>x.result.path);if(!outputs.length)return;try{const zipPath=await invoke('create_bundle',{paths:outputs});await saveOutput(zipPath,'CropStudio-batch.zip');}catch(e){toast(`Could not create ZIP: ${e}`);}}
async function runAutoCrop(){const item=active();if(!item||item.mediaType==='audio')return;saveActive();els.runAutoBtn.disabled=true;els.runAutoBtn.textContent='SCANNING…';try{const result=await invoke('auto_crop',{mediaId:item.mediaId,request:{mode:els.autoMode.value,windowSeconds:num(els.windowSeconds.value,8),aspectRatio:item.aspect||'free'}});markDirty(item);item.crop=result.crop;if(result.adaptivePlan)item.adaptivePlan=result.adaptivePlan;else item.adaptivePlan=null;renderCrop();els.autoSummary.textContent=`${result.sampleCount||0} sample(s) · ${result.kind==='adaptive'?'adaptive crop plan':'static crop suggestion'} applied.`;status('Auto crop applied. Review the frame before export.','success');persist();}catch(e){toast(`Auto crop failed: ${e}`);status(String(e),'error');}finally{els.runAutoBtn.disabled=false;els.runAutoBtn.textContent='RUN AUTO CROP →';}}
async function initEngine(){try{const r=await invoke('engine_status');els.engineStatus.textContent=r.ready?'RUST ENGINE · FFMPEG READY':'ENGINE MISSING · INSTALL FFMPEG';els.engineStatus.style.color=r.ready?'#4f791b':'#bd2828';if(!r.ready)toast('FFmpeg and FFprobe must be installed and available on PATH.');}catch(e){els.engineStatus.textContent='ENGINE STATUS UNKNOWN';}}
function wire(){[els.addFilesBtn,els.addFilesTop,els.emptyAddBtn].forEach(b=>b.addEventListener('click',chooseFiles));els.queueSearch.addEventListener('input',renderQueue);els.queueFilter.addEventListener('change',renderQueue);els.copyCropBtn.addEventListener('click',copyCropFrame);els.applyCropAllBtn.addEventListener('click',applyCropToAll);els.clearQueueBtn.addEventListener('click',async()=>{if(batchRunning)return;try{await invoke('clear_media',{mediaIds:queue.map(x=>x.mediaId)});}catch(e){toast(`Could not clean temporary media: ${e}`);}queue.length=0;activeId=null;localStorage.removeItem(STORE_KEY);renderActive();status('Queue cleared.');});els.previousMediaBtn.addEventListener('click',()=>moveSelection(-1));els.nextMediaBtn.addEventListener('click',()=>moveSelection(1));els.adjustCropBtn.addEventListener('click',()=>setCropMode('adjust'));els.drawCropBtn.addEventListener('click',()=>setCropMode('draw'));els.fullFrameBtn.addEventListener('click',setFullFrame);els.centerSquareBtn.addEventListener('click',setCenterSquare);els.aspectGrid.addEventListener('click',e=>{const b=e.target.closest('[data-aspect]');if(b)updateAspect(b.dataset.aspect);});document.querySelectorAll('[data-social]').forEach(b=>b.addEventListener('click',()=>applySocialPreset(b.dataset.social)));[els.cropX,els.cropY,els.cropW,els.cropH].forEach(x=>x.addEventListener('change',applyNumericCrop));els.trimStart.addEventListener('change',()=>setTrimStart(els.trimStart.value));els.trimEnd.addEventListener('change',()=>setTrimEnd(els.trimEnd.value));els.trimStartRange.addEventListener('input',()=>setTrimStart(els.trimStartRange.value));els.trimEndRange.addEventListener('input',()=>setTrimEnd(els.trimEndRange.value));els.resetTrimBtn.addEventListener('click',()=>{const item=active();if(!item)return;markDirty(item);item.trimStart=0;item.trimEnd=num(item.metadata.duration);renderTimeline(item);persist();});els.outputFormat.addEventListener('change',()=>{const item=active();if(item){markDirty(item);item.outputFormat=els.outputFormat.value;renderFormat(item);persist();}});[els.brightness,els.contrast,els.saturation,els.sharpness].forEach(el=>el.addEventListener('input',updateAdjustmentControl));[els.lutSelect,els.watermarkText,els.exportResolution,els.exportBitrate,els.exportFps,els.exportEncoder].forEach(el=>el.addEventListener('change',()=>{const item=active();if(item){markDirty(item);saveActive();}}));els.watermarkText.addEventListener('input',()=>{const item=active();if(item){markDirty(item);saveActive();}});els.resetLookBtn.addEventListener('click',resetLook);els.lutFileBtn.addEventListener('click',chooseLut);els.clearLutBtn.addEventListener('click',clearLut);els.exportBitrate.addEventListener('change',()=>updateExportEstimate());els.exportFps.addEventListener('change',()=>updateExportEstimate());els.compareBtn.addEventListener('pointerdown',e=>{e.preventDefault();beginCompare();});['pointerup','pointercancel','pointerleave'].forEach(name=>els.compareBtn.addEventListener(name,endCompare));window.addEventListener('pointerup',endCompare);els.processBtn.addEventListener('click',processActive);els.processBatchBtn.addEventListener('click',processBatch);els.saveResultBtn.addEventListener('click',()=>{const item=active();if(item?.result)saveOutput(item.result.path,item.result.filename).catch(e=>toast(String(e)));});els.downloadBatchBtn.addEventListener('click',saveBatch);els.autoCropBtn.addEventListener('click',()=>els.autoCropOptions.classList.toggle('hidden'));els.runAutoBtn.addEventListener('click',runAutoCrop);els.settingsBtn.addEventListener('click',()=>toast('CropStudio runs locally. FFmpeg handles media encoding; Rust manages the workflow.'));window.addEventListener('resize',()=>{const item=active();if(item)fitCanvas(item);});window.addEventListener('keydown',e=>{const typing=['INPUT','TEXTAREA','SELECT'].includes(document.activeElement?.tagName);if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='c'&&!typing){e.preventDefault();copyCropFrame();}if((e.ctrlKey||e.metaKey)&&e.shiftKey&&e.key.toLowerCase()==='v'&&!typing){e.preventDefault();applyCropToAll();}if(e.shiftKey&&e.key==='ArrowLeft'){e.preventDefault();moveSelection(-1);}if(e.shiftKey&&e.key==='ArrowRight'){e.preventDefault();moveSelection(1);}if(e.key==='Escape')setCropMode('adjust');if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='o'){e.preventDefault();chooseFiles();}});const stage=els.stage;stage.addEventListener('dragover',e=>{e.preventDefault();els.stageDropHint.classList.remove('hidden');});stage.addEventListener('dragleave',e=>{if(!stage.contains(e.relatedTarget))els.stageDropHint.classList.add('hidden');});stage.addEventListener('drop',e=>{e.preventDefault();els.stageDropHint.classList.add('hidden');const paths=[...(e.dataTransfer?.files||[])].map(f=>f.path).filter(Boolean);if(paths.length)addPaths(paths);else toast('Use the desktop file picker to add media.');});els.videoPreview.addEventListener('timeupdate',()=>{const item=active();if(item?.mediaType==='video'&&!compareSnapshot){els.filmstrip.querySelectorAll('.filmstrip-frame').forEach((b,i)=>b.classList.toggle('current',Math.abs(num(item.metadata.duration)*(i/Math.max(1,item.filmstrip.length-1))-els.videoPreview.currentTime)<Math.max(1,num(item.metadata.duration)/8)));}});setupCropGestures();}
async function initDragDrop(){try{await getCurrentWebview().onDragDropEvent(e=>{if(e.payload.type==='over'){els.stageDropHint.classList.remove('hidden');}else if(e.payload.type==='drop'){els.stageDropHint.classList.add('hidden');addPaths(e.payload.paths);}else{els.stageDropHint.classList.add('hidden');}});}catch(e){console.warn('Tauri drag-drop handler unavailable',e);}}
async function init(){wire();await initEngine();await restoreQueue();await initDragDrop();listen('cropstudio:progress',event=>{const p=event.payload;els.processProgress.classList.remove('hidden');els.processStage.textContent=p.stage||'Processing';els.processPercent.textContent=`${Math.round(p.progress||0)}%`;els.processFill.style.width=`${p.progress||0}%`;if(batchContext){const overall=Math.round(((batchContext.completed+(p.progress||0)/100)/batchContext.total)*100);els.batchPercent.textContent=`${overall}%`;els.batchFill.style.width=`${overall}%`;els.batchStage.textContent=`${batchContext.completed+1}/${batchContext.total} · ${p.stage||'Processing'}`;}});if(!queue.length)status('Waiting for media. Drop files or choose media.');}
init();
