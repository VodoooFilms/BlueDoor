export const MOBILE_PRIVACY_MODEL = "gliner-pii-small-int8";
const PRIVACY_MODELS = [MOBILE_PRIVACY_MODEL, "gliner-multi-pii-fp16", "qwen3-fast", "qwen3-q8", "bluedoor-pii-v1"];
export function privacyModel(mobile: boolean): string {
  let selected = localStorage.getItem("bluedoor-privacy-model");
  // Platform detection is asynchronous. Never erase a valid stored selection
  // while a component still has its initial desktop value.
  if (mobile && !localStorage.getItem("bluedoor-small-pii-default-v2")) {
    localStorage.setItem("bluedoor-small-pii-default-v2", "true");
  }
  if (!selected) return mobile ? MOBILE_PRIVACY_MODEL : "qwen3-fast";
  if (!PRIVACY_MODELS.includes(selected)) {
    selected = mobile ? MOBILE_PRIVACY_MODEL : "qwen3-fast";
    localStorage.setItem("bluedoor-privacy-model", selected);
  }
  return selected;
}
