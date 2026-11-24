import type { StringsFile } from "../types";

const applePrivacyKeys = {
  NSCameraUsageDescription: "apple_camera_usage_description",
  NSLocalNetworkUsageDescription: "apple_local_network_usage_description",
} as const;

function translation(doc: StringsFile, key: string, language: string): string {
  const value = doc.strings[key]?.translations?.[language];
  if (!value) throw new Error(`Missing system metadata for ${key} in ${language}`);
  return value;
}

function xmlText(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

export function appleInfoPlistStrings(doc: StringsFile, language: string): string {
  return Object.entries(applePrivacyKeys)
    .map(([key, entry]) => `${JSON.stringify(key)} = ${JSON.stringify(translation(doc, entry, language))};\n`)
    .join("");
}

export function syncApplePrivacyFallbacks(doc: StringsFile, plist: string): string {
  for (const [key, entry] of Object.entries(applePrivacyKeys)) {
    const pattern = new RegExp(`(<key>${key}</key>\\s*<string>)[\\s\\S]*?(</string>)`);
    if (!pattern.test(plist)) throw new Error(`Missing Info.plist key ${key}`);
    plist = plist.replace(pattern, (_match, start, end) =>
      start + xmlText(translation(doc, entry, doc.sourceLanguage)) + end);
  }
  return plist;
}

export function syncLinuxDesktopComments(doc: StringsFile, desktop: string): string {
  const comments = doc.supportedLanguages.map((language) => {
    const locale = language === doc.sourceLanguage ? "" : `[${language}]`;
    const value = translation(doc, "linux_desktop_description", language).replaceAll("\\", "\\\\").replaceAll("\n", "\\n");
    return `Comment${locale}=${value}`;
  });
  return desktop.replace(/^Comment=.*$(?:\nComment\[[^\]]+\]=.*$)*/m, () => comments.join("\n"));
}

export function syncLinuxMimeComments(doc: StringsFile, mime: string): string {
  const comments = doc.supportedLanguages.map((language) => {
    const locale = language === doc.sourceLanguage ? "" : ` xml:lang="${language}"`;
    const value = xmlText(translation(doc, "linux_mime_invitation_description", language));
    return `    <comment${locale}>${value}</comment>`;
  });
  return mime.replace(/^    <comment>.*<\/comment>$(?:\n    <comment xml:lang="[^"]+">.*<\/comment>$)*/m, () => comments.join("\n"));
}
