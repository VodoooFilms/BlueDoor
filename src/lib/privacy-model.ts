export const MOBILE_PRIVACY_MODEL = "gliner-multi-pii-fp16";
export function privacyModel(mobile: boolean): string {
  let selected = localStorage.getItem("bluedoor-privacy-model");
  // Apply the newly requested mobile default once, including upgraded installs.
  // Choices made after this upgrade remain explicit and are preserved.
  if (mobile && !localStorage.getItem("bluedoor-multi-pii-default-v1")) {
    selected = MOBILE_PRIVACY_MODEL;
    localStorage.setItem("bluedoor-privacy-model", selected);
    localStorage.setItem("bluedoor-multi-pii-default-v1", "true");
  }
  return selected || (mobile ? MOBILE_PRIVACY_MODEL : "qwen3-fast");
}
