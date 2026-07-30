import { commands } from '@bindings';

// Minimal valid 1x1 AVIF for capability probing.
const AVIF_PROBE =
  'data:image/avif;base64,AAAAIGZ0eXBhdmlmAAAAAGF2aWZtaWYxbWlhZk1BMUIAAADybWV0YQAAAAAAAAAoaGRscgAAAAAAAAAAcGljdAAAAAAAAAAAAAAAAGxpYmF2aWYAAAAADnBpdG0AAAAAAAEAAAAeaWxvYwAAAABEAAABAAEAAAABAAABGgAAAB0AAAAoaWluZgAAAAAAAQAAABppbmZlAgAAAAABAABhdjAxQ29sb3IAAAAAamlwcnAAAABLaXBjbwAAABRpc3BlAAAAAAAAAAEAAAABAAAAEHBpeGkAAAAAAwgICAAAAAxhdjFDgQ0MAAAAABNjb2xybmNseAACAAIABoAAAAAXaXBtYQAAAAAAAAABAAEEAQKDBAAAACVtZGF0EgAKCBgANogQEAwgMg8f8D///8WfhwB8+ErK42A=';

/**
 * Probe the WebView's AVIF decode capability and report the result to the
 * backend. Called once at startup. A failed or absent probe reports `false`,
 * which prevents the worker from starting any new conversion.
 */
export async function probeAvifCapability(): Promise<void> {
  let supported = false;
  try {
    supported = await new Promise<boolean>((resolve) => {
      const img = new Image();
      img.addEventListener('load', () => resolve(img.width > 0 && img.height > 0), {
        once: true,
      });
      img.addEventListener('error', () => resolve(false), { once: true });
      img.src = AVIF_PROBE;
    });
  } catch {
    supported = false;
  }
  try {
    await commands.imageReportAvifCapability(supported);
  } catch {
    // Ignore; the worker simply stays gated off.
  }
}
