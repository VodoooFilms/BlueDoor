export const MOBILE_PRIVACY_MODEL = "gliner-pii-small-int8";
export function privacyModel(mobile: boolean): string {
  let selected = localStorage.getItem("bluedoor-privacy-model");
  // Apply the newly requested mobile default once, including upgraded installs.
  // Choices made after this upgrade remain explicit and are preserved.
  if (mobile && !localStorage.getItem("bluedoor-small-pii-default-v2")) {
    selected = MOBILE_PRIVACY_MODEL;
    localStorage.setItem("bluedoor-privacy-model", selected);
    localStorage.setItem("bluedoor-small-pii-default-v2", "true");
  }
  const available = mobile
    ? [MOBILE_PRIVACY_MODEL, "gliner-multi-pii-fp16", "qwen3-fast"]
    : ["qwen3-fast", "qwen3-q8", "bluedoor-pii-v1"];
  if (selected && !available.includes(selected)) {
    selected = mobile ? MOBILE_PRIVACY_MODEL : "qwen3-fast";
    localStorage.setItem("bluedoor-privacy-model", selected);
  }
  return selected || (mobile ? MOBILE_PRIVACY_MODEL : "qwen3-fast");
}
