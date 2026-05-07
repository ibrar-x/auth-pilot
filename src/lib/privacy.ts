import type { AppSettings, PrivacyMaskStyle } from "../types";

export interface PrivacyMaskOptions {
  enabled: boolean;
  style: PrivacyMaskStyle;
  replacementText: string;
}

export function getPrivacyMaskOptions(settings: AppSettings | null | undefined): PrivacyMaskOptions {
  const replacementText = settings?.privacy_replacement_text?.trim() || "Hidden";

  return {
    enabled: settings?.privacy_mode_enabled ?? false,
    style: settings?.privacy_mask_style ?? "blur",
    replacementText,
  };
}

export function getMaskedText(value: string, options: PrivacyMaskOptions) {
  if (!options.enabled) {
    return { text: value, blur: false };
  }

  if (options.style === "replace") {
    return { text: options.replacementText, blur: false };
  }

  return { text: value, blur: true };
}
