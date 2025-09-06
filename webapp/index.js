// webapp/index.js
(() => {
  // ===== DOM =====
  const video = document.getElementById('video');
  const canvas = document.getElementById('canvas');
  const ctx = canvas.getContext('2d', { willReadFrequently: true });

  const btnStartCam    = document.getElementById('btnStartCam');
  const btnStartRec    = document.getElementById('btnStartRec');
  const btnStopRec     = document.getElementById('btnStopRec');
  const btnStartStream = document.getElementById('btnStartStream');
  const btnStopStream  = document.getElementById('btnStopStream');

  const cbEnableFilter = document.getElementById('cbEnableFilter');
  const selFilter      = document.getElementById('selFilter');
  const selBG          = document.getElementById('selBG');
  const rangeStrength  = document.getElementById('rangeStrength');
  const valStrength    = document.getElementById('valStrength');
  const led            = document.getElementById('led');

  const hostName   = document.getElementById('hostName');
  const hostMsg    = document.getElementById('hostMsg');
  const btnHostSend= document.getElementById('btnHostSend');
  const hostLog    = document.getElementById('hostLog');
  const shareBox   = document.getElementById('shareBox');
  const shareLinkEl= document.getElementById('shareLink');

  // ===== STATE =====
  let mediaStream = null;
  let rafId = 0;

  // Offscreens (direuse)
  const work = document.createElement('canvas'); // frame + filter
  const comp = document.createElement('canvas'); // compositing mask
  const tmp  = document.createElement('canvas'); // pixelate / scratch
  const wctx = work.getContext('2d', { willReadFrequently: true });
  const cctx = comp.getContext('2d', { willReadFrequently: true });
  const tctx = tmp.getContext('2d', { willReadFrequently: true });

  // Anime edges buffers (direuse ketika size berubah)
  let grayBuf = null, maskBuf = null, thickBuf = null, lastSizeKey = '';

  // Mediapipe
  let selfie = null;
  let latestMask = null;

  // Recording
  let recorder = null;
  let recChunks = [];

  // Streaming
  let ws = null;
  let sendTimer = 0;
  let roomId = null;

  // ===== Utils =====
  const qs = new URLSearchParams(location.search);
  function logHost(text, cls='') {
    const div = document.createElement('div');
    div.textContent = text;
    if (cls) div.className = cls;
    hostLog.appendChild(div);
    hostLog.scrollTop = hostLog.scrollHeight;
  }
  function setLed(on) { led.classList.toggle('on', !!on); }
  function getStrength() {
    const v = Number(rangeStrength?.value ?? 75);
    if (valStrength) valStrength.textContent = v;
    return v;
  }
  function getRoom() {
    let r = qs.get('room') || localStorage.getItem('live_room');
    if (!r) r = prompt('Nama room untuk streaming?', 'demo') || 'demo';
    localStorage.setItem('live_room', r);
    return r;
  }
  function ensureSize() {
    const w = video.videoWidth || 1280;
    const h = video.videoHeight || 720;
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w; canvas.height = h;
      work.width = comp.width = tmp.width = w;
      work.height = comp.height = tmp.height = h;

      const key = `${w}x${h}`;
      if (key !== lastSizeKey) {
        grayBuf  = new Uint8ClampedArray(w * h);
        maskBuf  = new Uint8Array(w * h);
        thickBuf = new Uint8Array(w * h);
        lastSizeKey = key;
      }
    }
  }
  function supportsWebP() {
    try {
      return canvas.toDataURL('image/webp').startsWith('data:image/webp');
    } catch { return false; }
  }

  // ===== Filters =====
  function drawBG(kind, w, h) {
    if (kind === 'origin') {
      ctx.drawImage(video, 0, 0, w, h);
      return;
    }
    if (kind === 'gray') {
      ctx.drawImage(video, 0, 0, w, h);
      const img = ctx.getImageData(0, 0, w, h), d = img.data;
      for (let i=0;i<d.length;i+=4){
        const g = 0.299*d[i] + 0.587*d[i+1] + 0.114*d[i+2];
        d[i]=d[i+1]=d[i+2]=g;
      }
      ctx.putImageData(img, 0, 0);
      return;
    }
    if (kind === 'pixel') {
      const scale = 80;
      tctx.imageSmoothingEnabled = false;
      const tw = Math.max(1, (w/scale)|0);
      const th = Math.max(1, (h/scale)|0);
      tctx.clearRect(0,0,tw,th);
      tctx.drawImage(video, 0, 0, w, h, 0, 0, tw, th);
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(tmp, 0, 0, tw, th, 0, 0, w, h);
      ctx.imageSmoothingEnabled = true;
      return;
    }
    if (kind === 'blur') {
      ctx.filter = 'blur(10px)';
      ctx.drawImage(video, 0, 0, w, h);
      ctx.filter = 'none';
      return;
    }
    if (kind === 'grad') {
      const g = ctx.createLinearGradient(0,0,w,h);
      g.addColorStop(0, '#191970'); g.addColorStop(1, '#8a2be2');
      ctx.fillStyle = g; ctx.fillRect(0,0,w,h);
      return;
    }
  }

  function animeProcess(dstCtx, w, h, strength=80) {
    // Ambil frame dari video ke work terlebih dulu
    wctx.drawImage(video, 0, 0, w, h);
    const img = wctx.getImageData(0,0,w,h);
    const d = img.data;

    // Posterize
    const levels = Math.max(3, Math.round(4 + (strength/100)*10)); // 4..14
    const step = Math.max(2, Math.floor(256/levels));
    for (let i=0;i<d.length;i+=4){
      d[i]   = Math.floor(d[i]/step)*step;
      d[i+1] = Math.floor(d[i+1]/step)*step;
      d[i+2] = Math.floor(d[i+2]/step)*step;
    }

    // Contrast + Saturation
    const sat = 1.25 + (strength/100)*0.75;
    const con = 1.10 + (strength/100)*0.45;
    for (let i=0;i<d.length;i+=4){
      d[i]   = Math.max(0, Math.min(255, (d[i]-128)*con + 128));
      d[i+1] = Math.max(0, Math.min(255, (d[i+1]-128)*con + 128));
      d[i+2] = Math.max(0, Math.min(255, (d[i+2]-128)*con + 128));
      const g = 0.299*d[i] + 0.587*d[i+1] + 0.114*d[i+2];
      d[i]   = Math.max(0, Math.min(255, g + (d[i]-g)*sat));
      d[i+1] = Math.max(0, Math.min(255, g + (d[i+1]-g)*sat));
      d[i+2] = Math.max(0, Math.min(255, g + (d[i+2]-g)*sat));
    }

    // Edge (Sobel) + dilasi tipis — pakai buffer reuse
    for (let i=0,p=0;i<d.length;i+=4,p++){
      grayBuf[p] = 0.299*d[i] + 0.587*d[i+1] + 0.114*d[i+2];
    }
    const kx=[-1,0,1,-2,0,2,-1,0,1], ky=[-1,-2,-1,0,0,0,1,2,1];
    const thr = 180 - Math.round((strength/100)*70);
    const W = w, H = h;

    maskBuf.fill(0);
    for (let y=1;y<H-1;y++){
      for (let x=1;x<W-1;x++){
        let gx=0, gy=0, k=0;
        const idx = y*W + x;
        for (let yy=-1; yy<=1; yy++) for (let xx=-1; xx<=1; xx++){
          const v = grayBuf[idx + yy*W + xx];
          gx += kx[k]*v; gy += ky[k]*v; k++;
        }
        const mag = Math.hypot(gx, gy);
        maskBuf[idx] = mag > thr ? 1 : 0;
      }
    }
    thickBuf.fill(0);
    for (let y=1;y<H-1;y++){
      for (let x=1;x<W-1;x++){
        let on=0;
        for (let yy=-1;yy<=1;yy++){
          for (let xx=-1;xx<=1;xx++){
            if (maskBuf[(y+yy)*W + (x+xx)]) { on=1; break; }
          }
          if (on) break;
        }
        thickBuf[y*W + x] = on;
      }
    }
    for (let y=0;y<H;y++){
      for (let x=0;x<W;x++){
        if (thickBuf[y*W + x]) {
          const o = (y*W + x) * 4;
          d[o]=d[o+1]=d[o+2]=18;
        }
      }
    }

    dstCtx.putImageData(img, 0, 0);

    // Soft glow
    dstCtx.save();
    dstCtx.globalAlpha = 0.12 + (strength/100)*0.08;
    dstCtx.filter = `blur(${Math.round(2 + (strength/100)*3)}px)`;
    dstCtx.drawImage(dstCtx.canvas, 0, 0);
    dstCtx.filter = 'none';
    dstCtx.globalAlpha = 1;
    dstCtx.restore();
  }

  function avatarProcess(dstCtx, w, h, strength=75){
    wctx.drawImage(video, 0, 0, w, h);
    const img = wctx.getImageData(0,0,w,h), d=img.data;
    const poster = Math.max(8, Math.round(256/Math.max(2,Math.round(strength/8))));
    for (let i=0;i<d.length;i+=4){
      d[i]  = Math.floor(d[i]  /poster)*poster;
      d[i+1]= Math.floor(d[i+1]/poster)*poster;
      d[i+2]= Math.floor(d[i+2]/poster)*poster;
    }
    const sat=1.2+(strength/100)*0.6, con=1.1+(strength/100)*0.4;
    for (let i=0;i<d.length;i+=4){
      d[i]   = Math.max(0, Math.min(255,(d[i]-128)*con+128));
      d[i+1] = Math.max(0, Math.min(255,(d[i+1]-128)*con+128));
      d[i+2] = Math.max(0, Math.min(255,(d[i+2]-128)*con+128));
      const g=0.299*d[i]+0.587*d[i+1]+0.114*d[i+2];
      d[i]   = Math.min(255, g+(d[i]-g)*sat);
      d[i+1] = Math.min(255, g+(d[i+1]-g)*sat);
      d[i+2] = Math.min(255, g+(d[i+2]-g)*sat);
      d[i]   = Math.min(255, d[i]   * 1.06);
      d[i+2] = Math.min(255, d[i+2] * (1.10 + (strength/100)*0.2));
    }
    dstCtx.putImageData(img,0,0);
  }

  function simpleFilter(dstCtx, w, h, kind, strength){
    // draw base
    dstCtx.drawImage(video, 0, 0, w, h);
    if (kind==='beautify'){
      dstCtx.filter=`blur(${Math.round(2+(strength/100)*8)}px) brightness(${1.1+(strength/100)*0.4}) contrast(${1+(strength/100)*0.25})`;
      dstCtx.drawImage(work,0,0);
      dstCtx.filter='none';
      return;
    }
    if (kind==='pixelate'){
      const s = Math.max(6, Math.round(strength/2));
      const tw = Math.max(1, (w/s)|0), th = Math.max(1, (h/s)|0);
      tctx.imageSmoothingEnabled=false;
      tctx.clearRect(0,0,tw,th);
      tctx.drawImage(work,0,0,w,h,0,0,tw,th);
      dstCtx.imageSmoothingEnabled=false;
      dstCtx.drawImage(tmp,0,0,tw,th,0,0,w,h);
      dstCtx.imageSmoothingEnabled=true;
      return;
    }
    if (kind==='gray' || kind==='invert' || kind==='sepia' || kind==='vignette'){
      const img=dstCtx.getImageData(0,0,w,h), d=img.data;
      if (kind==='gray'){
        for (let i=0;i<d.length;i+=4){
          const g=0.3*d[i]+0.59*d[i+1]+0.11*d[i+2]; d[i]=d[i+1]=d[i+2]=g;
        }
        dstCtx.putImageData(img,0,0);
      } else if (kind==='invert'){
        for (let i=0;i<d.length;i+=4){ d[i]=255-d[i]; d[i+1]=255-d[i+1]; d[i+2]=255-d[i+2]; }
        dstCtx.putImageData(img,0,0);
      } else if (kind==='sepia'){
        for (let i=0;i<d.length;i+=4){
          const r=d[i],g=d[i+1],b=d[i+2];
          d[i]=Math.min(255,0.393*r+0.769*g+0.189*b);
          d[i+1]=Math.min(255,0.349*r+0.686*g+0.168*b);
          d[i+2]=Math.min(255,0.272*r+0.534*g+0.131*b);
        }
        dstCtx.putImageData(img,0,0);
      } else if (kind==='vignette'){
        dstCtx.putImageData(img,0,0);
        dstCtx.save();
        const g=dstCtx.createRadialGradient(w/2,h/2,Math.max(w,h)*0.4,w/2,h/2,Math.max(w,h)*0.8);
        g.addColorStop(0,'rgba(0,0,0,0)'); g.addColorStop(1,'rgba(0,0,0,0.7)');
        dstCtx.fillStyle=g; dstCtx.fillRect(0,0,w,h); dstCtx.restore();
      }
    }
  }

  // ===== Camera & Segmentation =====
  async function startCamera() {
    if (mediaStream) return;
    try {
      mediaStream = await navigator.mediaDevices.getUserMedia({
        video: { width: { ideal: 1280 }, height: { ideal: 720 }, facingMode: 'user' },
        audio: false
      });
    } catch (e) {
      alert('Gagal mengakses kamera: ' + e.message);
      return;
    }

    video.srcObject = mediaStream;
    await video.play();

    ensureSize();
    await initSegmentation();

    // enable buttons
    btnStartCam.disabled = true;
    btnStartRec.disabled = false;
    btnStartStream.disabled = false;

    cancelAnimationFrame(rafId);
    rafId = requestAnimationFrame(drawLoop);
  }

  async function initSegmentation() {
    latestMask = null;
    if (!window.SelfieSegmentation) return; // CDN belum dimuat → jalan tanpa mask

    selfie = new SelfieSegmentation({
      locateFile: (file) =>
        `https://cdn.jsdelivr.net/npm/@mediapipe/selfie_segmentation/${file}`,
    });
    selfie.setOptions({ modelSelection: 1 });
    selfie.onResults((res) => {
      // res.segmentationMask adalah HTMLCanvasElement
      latestMask = res.segmentationMask || null;
    });
  }

  async function drawLoop() {
    rafId = requestAnimationFrame(drawLoop);
    if (!video.videoWidth) return;
    ensureSize();

    const w = canvas.width, h = canvas.height;
    const strength = getStrength();

    // 1) Background
    drawBG(selBG.value, w, h);

    // 2) Update mask (async tapi throttle ringan)
    if (selfie && (!drawLoop._tick || Date.now() - drawLoop._tick > 60)) {
      drawLoop._tick = Date.now();
      selfie.send({ image: video });
    }

    // 3) Siapkan frame terproses di 'work'
    wctx.clearRect(0,0,w,h);
    if (selFilter.value === 'anime') {
      animeProcess(wctx, w, h, strength);
    } else if (selFilter.value === 'avatar') {
      avatarProcess(wctx, w, h, strength);
    } else {
      simpleFilter(wctx, w, h, selFilter.value, strength);
    }

    // 4) Komposit (mask only if filter enabled & mask ada)
    if (cbEnableFilter.checked && latestMask) {
      cctx.clearRect(0,0,w,h);
      // (mask ∧ work) di atas background
      cctx.drawImage(latestMask, 0, 0, w, h);
      cctx.globalCompositeOperation = 'source-in';
      cctx.drawImage(work, 0, 0, w, h);
      cctx.globalCompositeOperation = 'source-over';
      ctx.drawImage(comp, 0, 0, w, h);
    } else {
      // Tanpa mask → full frame
      ctx.drawImage(work, 0, 0, w, h);
    }
  }

  // ===== Recording =====
  function startRec() {
    if (recorder) return;
    const stream = canvas.captureStream(25);
    recorder = new MediaRecorder(stream, { mimeType: 'video/webm;codecs=vp9' });
    recChunks.length = 0;
    recorder.ondataavailable = (e) => { if (e.data && e.data.size) recChunks.push(e.data); };
    recorder.onstop = () => {
      const blob = new Blob(recChunks, { type: 'video/webm' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `record_${new Date().toISOString().replace(/[-:T.Z]/g,'').slice(0,14)}.webm`;
      a.click();
      URL.revokeObjectURL(url);
      recChunks.length = 0;
    };
    recorder.start();
    setLed(true);
    btnStartRec.disabled = true;
    btnStopRec.disabled = false;
  }
  function stopRec() {
    if (!recorder) return;
    recorder.stop();
    recorder = null;
    setLed(false);
    btnStartRec.disabled = false;
    btnStopRec.disabled = true;
  }

  // ===== Streaming WS =====
  function startStream() {
    if (ws) return;
    roomId = getRoom();
    const originWs = location.origin.replace(/^http/, 'ws');
    ws = new WebSocket(`${originWs}/ws/${encodeURIComponent(roomId)}`);

    ws.onopen = () => {
      logHost(`Streaming started. Room: ${roomId}`, 'sys');
      const url = `${location.origin}/live/${encodeURIComponent(roomId)}`;
      shareLinkEl.textContent = url;
      shareBox.style.display = 'inline-flex';

      const useWebP = supportsWebP();
      // kirim frame periodik (cek backpressure)
      sendTimer = setInterval(() => {
        if (!ws || ws.readyState !== 1) return;
        if (ws.bufferedAmount > (1<<20)) return; // >1MB → skip frame

        const mime = useWebP ? 'image/webp' : 'image/jpeg';
        const q = useWebP ? 0.6 : 0.7;
        try {
          const dataUrl = canvas.toDataURL(mime, q);
          ws.send(JSON.stringify({ t:'f', room: roomId, d: dataUrl }));
        } catch {}
      }, 80); // ~12.5 fps
      btnStartStream.disabled = true;
      btnStopStream.disabled = false;
    };

    ws.onmessage = (ev) => {
      try {
        const msg = JSON.parse(ev.data);
        if (msg.t === 'c') logHost(`${msg.user||'viewer'}: ${msg.text}`);
      } catch {}
    };

    ws.onclose = () => {
      logHost('Koneksi WS tertutup', 'sys');
      stopStream();
    };
    ws.onerror = () => logHost('WS error', 'sys');

    // host chat
    btnHostSend.onclick = () => {
      if (!ws || ws.readyState !== 1) return;
      const user = (hostName.value || 'host').trim();
      const text = (hostMsg.value || '').trim();
      if (!text) return;
      ws.send(JSON.stringify({ t:'c', room: roomId, user, text }));
      logHost(`(me) ${user}: ${text}`, 'me');
      hostMsg.value = '';
    };
  }
  function stopStream() {
    if (sendTimer) clearInterval(sendTimer);
    sendTimer = 0;
    try { ws && ws.close(); } catch {}
    ws = null;
    btnStartStream.disabled = false;
    btnStopStream.disabled = true;
    logHost('Streaming stopped', 'sys');
  }

  // ===== Bind UI (satu kali, tak ada onclick ganda) =====
  btnStartCam?.addEventListener('click', startCamera);
  btnStartRec?.addEventListener('click', startRec);
  btnStopRec?.addEventListener('click', stopRec);
  btnStartStream?.addEventListener('click', startStream);
  btnStopStream?.addEventListener('click', stopStream);
  rangeStrength?.addEventListener('input', () => { if (valStrength) valStrength.textContent = rangeStrength.value; });

  // ===== Cleanup =====
  window.addEventListener('beforeunload', () => {
    try { stopStream(); } catch {}
    try { stopRec(); } catch {}
    if (rafId) cancelAnimationFrame(rafId);
    if (mediaStream) mediaStream.getTracks().forEach(t => t.stop());
  });
})();
