///<reference types="svelte" />
;
import { t } from "../lib/i18n.svelte.ts";

;type $$ComponentProps =  { onresult: (text: string) => void; oncancel: () => void };function $$render() {

  

  let { onresult, oncancel }:/*Ωignore_startΩ*/$$ComponentProps/*Ωignore_endΩ*/ = $props();
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
;
async () => {

 { svelteHTML.createElement("div", { "class":`scanner`,});
  
   { const $$_video1 = svelteHTML.createElement("video", {  "playsinline":true,"muted":true,});video = $$_video1; }
   { svelteHTML.createElement("div", {   "class":`frame`,"aria-hidden":`true`,}); }
  if(error){ { svelteHTML.createElement("p", { "class":`err`,});error; }}
   { svelteHTML.createElement("button", {   "class":`btn`,"onclick":oncancel,});t("Cancel"); }
 }


};
return { props: {} as any as $$ComponentProps, exports: {}, bindings: __sveltets_$$bindings(''), slots: {}, events: {} }}
const Scanner__SvelteComponent_ = __sveltets_2_fn_component($$render());
/*Ωignore_startΩ*/type Scanner__SvelteComponent_ = ReturnType<typeof Scanner__SvelteComponent_>;
/*Ωignore_endΩ*/export default Scanner__SvelteComponent_;