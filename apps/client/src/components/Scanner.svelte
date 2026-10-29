<script lang="ts">
  import { t } from "../lib/i18n.svelte.ts";

  let { onresult, oncancel }: { onresult: (text: string) => void; oncancel: () => void } = $props();
  let video: HTMLVideoElement;
  let error = $state("");

  $effect(() => {
    let stream: MediaStream | null = null;
    let stopped = false;
    let detector: any = null;
    let canvas: HTMLCanvasElement | null = null;
    let jsqr: ((d: Uint8ClampedArray, w: number, h: number) => { data: string } | null) | null = null;

    (async () => {
      try {
        stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: "environment" }, audio: false });
      } catch {
        error = t("Camera is not available. Allow camera access, or paste the pairing link instead.");
        return;
      }
      if (stopped) return stream.getTracks().forEach((tr) => tr.stop());
      video.srcObject = stream;
      await video.play().catch(() => {});
      if ("BarcodeDetector" in globalThis) detector = new (globalThis as any).BarcodeDetector({ formats: ["qr_code"] });
      else jsqr = (await import("jsqr")).default as any;
      const tick = async () => {
        if (stopped) return;
        try {
          if (video.readyState >= 2) {
            let text: string | undefined;
            if (detector) {
              text = (await detector.detect(video))[0]?.rawValue;
            } else if (jsqr) {
              canvas ??= document.createElement("canvas");
              const w = (canvas.width = Math.min(640, video.videoWidth));
              const h = (canvas.height = Math.round((video.videoHeight / video.videoWidth) * w));
              const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
              ctx.drawImage(video, 0, 0, w, h);
              text = jsqr(ctx.getImageData(0, 0, w, h).data, w, h)?.data;
            }
            if (text) {
              onresult(text);
              return;
            }
          }
        } catch {
          /* keep scanning */
        }
        setTimeout(tick, 200);
      };
      void tick();
    })();

    return () => {
      stopped = true;
      stream?.getTracks().forEach((tr) => tr.stop());
    };
  });
</script>

<div class="scanner">
  <!-- svelte-ignore a11y_media_has_caption -->
  <video bind:this={video} playsinline muted></video>
  <div class="frame" aria-hidden="true"></div>
  {#if error}<p class="err">{error}</p>{/if}
  <button class="btn" onclick={oncancel}>{t("Cancel")}</button>
</div>

<style>
  .scanner {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    position: relative;
  }
  video {
    width: min(100%, 420px);
    aspect-ratio: 1;
    object-fit: cover;
    border-radius: 16px;
    background: var(--surface);
  }
  .frame {
    position: absolute;
    top: 0;
    width: min(100%, 420px);
    aspect-ratio: 1;
    border: 2px solid var(--accent);
    border-radius: 16px;
    pointer-events: none;
  }
  .err {
    color: var(--danger-text);
    text-align: center;
  }
</style>
