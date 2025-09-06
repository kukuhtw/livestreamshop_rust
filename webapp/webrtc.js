// /static/js/webrtc.js
(function () {
  'use strict';

  // ---------- Config ----------
  const ICE = [{ urls: 'stun:stun.l.google.com:19302' }];

  function getRoom() {
    const q = new URL(location.href).searchParams.get('room');
    return (q || 'main').replace(/[^\w-]/g, '');
  }
  let ROOM_NAME = getRoom();

  // ---------- State ----------
  let pc = null;
  let dc = null;
  let wsSig = null;
  let streamOut = null;
  let lastOfferSDP = null;

  // ---------- Safe toast ----------
  const toast =
    window.toast ||
    function (msg, ms = 2400) {
      const t = document.createElement('div');
      t.className =
        'toast fixed bottom-4 left-1/2 -translate-x-1/2 px-4 py-2 rounded-lg shadow text-white bg-black/80 z-50';
      t.textContent = msg;
      document.body.appendChild(t);
      setTimeout(() => t.remove(), ms);
    };

  // ---------- UI helpers ----------
  function updateShareLink(on) {
    const box = document.getElementById('shareBox');
    const link = document.getElementById('shareLink');
    if (!box || !link) return;
    if (on) {
      const url = `${location.origin}/static/livepage.html?room=${encodeURIComponent(
        ROOM_NAME
      )}`;
      link.textContent = url;
      link.href = url;
      box.style.display = '';
    } else {
      box.style.display = 'none';
      link.textContent = '';
      link.removeAttribute('href');
    }
  }

  function appendChatLine(prefix, text, me = false) {
    const log = document.getElementById('hostLog');
    if (!log) return;
    const div = document.createElement('div');
    div.textContent = `${prefix} ${text}`;
    if (me) div.className = 'me';
    log.appendChild(div);
    log.scrollTop = log.scrollHeight;
  }

  // ---------- Peer setup ----------
  async function makePeer(stream) {
    pc = new RTCPeerConnection({ iceServers: ICE });
    stream.getTracks().forEach((t) => pc.addTrack(t, stream));

    // DataChannel host -> viewer
    dc = pc.createDataChannel('chat');
    dc.onopen = () => console.log('[HOST] DataChannel open');
    dc.onmessage = (e) => appendChatLine('(viewer)', e.data, false);

    pc.onicecandidate = (ev) => {
      if (ev.candidate) {
        try {
          wsSig?.send(JSON.stringify({ t: 'ice', candidate: ev.candidate }));
        } catch (_) {}
      }
    };
  }

  async function makeAndSendOffer() {
    if (!pc) return;
    // Biarkan default (JANGAN mematikan audio), supaya audio ikut dinego
    const offer = await pc.createOffer();
    await pc.setLocalDescription(offer);
    lastOfferSDP = offer.sdp;
    try {
      wsSig?.send(JSON.stringify({ t: 'offer', sdp: offer.sdp }));
      console.log('[HOST] offer sent');
    } catch (_) {}
  }

  // ---------- Start / Stop ----------
  async function startWebRTC_AsHost() {
    const canvas = document.getElementById('canvas');
    if (!canvas || !canvas.captureStream) {
      toast('Browser tidak mendukung captureStream dari canvas');
      return;
    }

    // Ambil stream dari canvas (pastikan kamera/filter sudah berjalan)
    streamOut = canvas.captureStream(30);

    // Tambah mic host (audio track)
    try {
      const mic = await navigator.mediaDevices.getUserMedia({
        audio: {
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
      });
      mic.getAudioTracks().forEach((t) => streamOut.addTrack(t));
      console.log('[HOST] mic added:', mic.getAudioTracks().length, 'track(s)');
    } catch (e) {
      console.warn('[HOST] mic unavailable:', e);
      toast('Mic tidak tersedia / belum diizinkan');
    }

    await makePeer(streamOut);

    // Signaling WS
    const origin = location.origin.replace(/^http/, 'ws');
    wsSig = new WebSocket(`${origin}/ws/${ROOM_NAME}`);

    wsSig.onopen = async () => {
      console.log('[HOST] WS open');
      await makeAndSendOffer();
      updateShareLink(true);
      toast('WebRTC streaming dimulai');
    };

    wsSig.onmessage = async (ev) => {
      try {
        const msg = JSON.parse(ev.data);

        // viewer mengabarkan masuk -> re-offer
        if (msg.t === 'sys' && msg.text === 'viewer_enter') {
          console.log('[HOST] viewer_enter → re-send offer');
          if (lastOfferSDP && pc?.signalingState !== 'closed') {
            wsSig.send(JSON.stringify({ t: 'offer', sdp: lastOfferSDP }));
          } else {
            await makeAndSendOffer();
          }
        } else if (msg.t === 'answer' && msg.sdp) {
          await pc.setRemoteDescription(
            new RTCSessionDescription({ type: 'answer', sdp: msg.sdp })
          );
          console.log('[HOST] answer applied');
        } else if (msg.t === 'ice' && msg.candidate) {
          try {
            await pc.addIceCandidate(new RTCIceCandidate(msg.candidate));
          } catch (_) {}
        }
      } catch (_) {}
    };

    wsSig.onclose = () => console.log('[HOST] WS closed');
  }

  function stopWebRTC_AsHost() {
    try {
      dc?.close();
    } catch (_) {}
    try {
      pc?.close();
    } catch (_) {}
    try {
      if (wsSig?.readyState === 1) wsSig.close();
    } catch (_) {}
    if (streamOut) {
      streamOut.getTracks().forEach((t) => t.stop());
      streamOut = null;
    }
    pc = null;
    dc = null;
    wsSig = null;
    lastOfferSDP = null;
    updateShareLink(false);
    toast('WebRTC streaming dihentikan');
  }

  // ---------- Chat send ----------
  function sendHostChat() {
    const nameEl = document.getElementById('hostName');
    const msgEl = document.getElementById('hostMsg');
    const name = (nameEl?.value || 'host').trim();
    const text = (msgEl?.value || '').trim();
    if (!text) return;

    if (dc && dc.readyState === 'open') {
      dc.send(`${name}: ${text}`);
      appendChatLine('(me)', `${name}: ${text}`, true);
      if (msgEl) msgEl.value = '';
    } else {
      // Fallback via WS events global (optional, agar kompatibel dengan sistem lama)
      const origin = location.origin.replace(/^http/, 'ws');
      const wsev = new WebSocket(`${origin}/ws/_events`);
      wsev.onopen = () => {
        wsev.send(JSON.stringify({ t: 'c', room: ROOM_NAME, user: name, text }));
        wsev.close();
      };
      if (msgEl) msgEl.value = '';
    }
  }

  // ---------- Wire buttons ----------
  function wireButtons() {
    document.getElementById('btnStartStream')?.addEventListener('click', async () => {
      // Minta nama room (prefill dari ROOM_NAME saat ini / ?room=)
      const raw = prompt(
        'Masukkan nama room (huruf/angka/garis/underscore):',
        ROOM_NAME || 'main'
      );
      if (!raw) return; // batal
      // Sanitasi: hanya [A-Za-z0-9_-]
      const cleaned = String(raw).replace(/[^\w-]/g, '');
      if (!cleaned) {
        toast('Nama room tidak valid');
        return;
      }

      // Jika sedang aktif, hentikan dulu sebelum ganti room
      if (window.LiveShopRTC?._isActive?.()) {
        try {
          stopWebRTC_AsHost();
        } catch (_) {}
      }

      ROOM_NAME = cleaned;

      // Perbarui URL agar konsisten
      try {
        const u = new URL(location.href);
        u.searchParams.set('room', ROOM_NAME);
        history.replaceState(null, '', u.toString());
      } catch (_) {}

      await startWebRTC_AsHost();
    });

    document
      .getElementById('btnStopStream')
      ?.addEventListener('click', stopWebRTC_AsHost);
    document
      .getElementById('btnHostSend')
      ?.addEventListener('click', sendHostChat);

    // expose untuk debugging manual di console
    window.LiveShopRTC = {
      startWebRTC_AsHost,
      stopWebRTC_AsHost,
      _isActive: () => !!(pc || wsSig || streamOut),
    };

    window.addEventListener('beforeunload', () => {
      try {
        stopWebRTC_AsHost();
      } catch (_) {}
    });
  }

  // ---------- Init ----------
  document.addEventListener('DOMContentLoaded', wireButtons);
})();
