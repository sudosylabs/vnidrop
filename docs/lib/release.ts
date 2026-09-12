const GITHUB_REPO = "sudosylabs/vnidrop";

export const githubLatestUrl = `https://github.com/${GITHUB_REPO}/releases/latest`;
export const githubRepoUrl = `https://github.com/${GITHUB_REPO}`;
export const githubPreviewUrl = `${githubRepoUrl}/releases?q=preview-&expanded=true`;
export const windowsStoreUrl = "https://apps.microsoft.com/detail/9NJ5Q0FG7TGL";
export const homebrewInstall = "brew install --cask sudosylabs/vnidrop/vnidrop";

const manifestUrl = `https://github.com/${GITHUB_REPO}/releases/latest/download/release-manifest.json`;

type ManifestFile = {
  name: string;
  sha256: string;
  bytes: number;
};

type Manifest = {
  productVersion: string;
  releaseChannel: string;
  tag: string;
  files: ManifestFile[];
  downloads?: Partial<Record<"windows" | "linux" | "android" | "macos", {
    version: string;
    tag: string;
    files: ManifestFile[];
  }>>;
};

export type ReleaseAsset = {
  version: string;
  tag: string;
  checksumsUrl: string;
  name: string;
  url: string;
  bytes: number;
  sha256?: string;
};

export type PreviewRelease = LatestRelease & {
  number: number;
  publishedAt: string;
  manifestUrl: string;
};

export type LatestRelease = {
  version: string;
  channel: string;
  tag: string;
  tagUrl: string;
  checksumsUrl: string;
  dmg?: ReleaseAsset;
  deb?: ReleaseAsset;
  rpm?: ReleaseAsset;
  apk?: ReleaseAsset;
  windowsExe?: ReleaseAsset;
};

export function assetDownloadUrl(tag: string, name: string): string {
  return `https://github.com/${GITHUB_REPO}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(name)}`;
}

function toAsset(tag: string, file: ManifestFile): ReleaseAsset {
  return {
    version: tag.slice(1),
    tag,
    checksumsUrl: assetDownloadUrl(tag, "SHA256SUMS"),
    name: file.name,
    url: assetDownloadUrl(tag, file.name),
    bytes: file.bytes,
    sha256: file.sha256,
  };
}

function findFile(
  tag: string,
  files: ManifestFile[],
  pattern: RegExp,
): ReleaseAsset | undefined {
  const match = files.find((file) => pattern.test(file.name));
  return match ? toAsset(tag, match) : undefined;
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) {
    return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  }
  if (bytes >= 1_000_000) {
    return `${(bytes / 1_000_000).toFixed(1)} MB`;
  }
  if (bytes >= 1_000) {
    return `${(bytes / 1_000).toFixed(1)} KB`;
  }
  return `${bytes} B`;
}

export async function loadLatestRelease(): Promise<LatestRelease> {
  const response = await fetch(manifestUrl, {
    cache: process.env.NODE_ENV === "development" ? "no-store" : "force-cache",
  });
  if (!response.ok) {
    throw new Error(`Failed to load GitHub release manifest (${response.status})`);
  }

  const manifest = (await response.json()) as Manifest;
  return releaseFromManifest(manifest);
}

export function releaseFromManifest(manifest: Manifest): LatestRelease {
  const files = manifest.files ?? [];
  const tag = manifest.tag;
  const versionPattern = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
  if (manifest.releaseChannel === "preview" || !versionPattern.test(manifest.productVersion) || tag !== `v${manifest.productVersion}`) {
    throw new Error("The public release manifest must identify a normal version tag");
  }
  function download(platform: "windows" | "linux" | "android" | "macos", pattern: RegExp) {
    const entry = manifest.downloads?.[platform];
    if (!entry) return findFile(tag, files, pattern);
    if (!versionPattern.test(entry.version) || entry.tag !== `v${entry.version}` || !Array.isArray(entry.files)) {
      throw new Error(`Invalid ${platform} download identity`);
    }
    const current = manifest.productVersion.split(".").map(Number);
    const candidate = entry.version.split(".").map(Number);
    const differing = candidate.findIndex((value, index) => value !== current[index]);
    if (differing >= 0 && candidate[differing] > current[differing]) throw new Error("Download version is newer than its manifest");
    if (entry.files.some((file) => /[/\\]/.test(file.name) || !file.name.includes(entry.version) ||
        !/^[a-f0-9]{64}$/.test(file.sha256) || !Number.isSafeInteger(file.bytes) || file.bytes <= 0)) {
      throw new Error(`Invalid ${platform} download metadata`);
    }
    return findFile(entry.tag, entry.files, pattern);
  }
  return {
    version: manifest.productVersion,
    channel: manifest.releaseChannel,
    tag,
    tagUrl: `https://github.com/${GITHUB_REPO}/releases/tag/${encodeURIComponent(tag)}`,
    checksumsUrl: assetDownloadUrl(tag, "SHA256SUMS"),
    dmg: download("macos", /^VniDrop-.+\.dmg$/),
    deb: download("linux", /\.deb$/),
    rpm: download("linux", /\.rpm$/),
    apk: download("android", /play-universal\.apk$/),
    windowsExe: download("windows", /^VniDrop_.+_x64\.exe$/),
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function selectLatestPreview(data: unknown): PreviewRelease | null {
  if (!Array.isArray(data)) throw new Error("Invalid GitHub releases response");

  const previews: PreviewRelease[] = [];
  for (const release of data) {
    if (!isRecord(release) || release.draft !== false || release.prerelease !== true ||
      typeof release.tag_name !== "string" || typeof release.published_at !== "string" ||
      !Number.isFinite(Date.parse(release.published_at)) || !Array.isArray(release.assets)) continue;

    const identity = /^preview-((?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*))-([1-9]\d*)$/.exec(release.tag_name);
    if (!identity || Number(identity[2]) > 2_100_000_000) continue;
    const [, version, number] = identity;
    const label = `${version}-preview.${number}`;
    const tag = release.tag_name;
    const assets = release.assets.filter(isRecord);
    const asset = (name: string): ReleaseAsset | undefined => {
      const matches = assets.filter((file) => file.name === name && file.state === "uploaded" &&
        typeof file.size === "number" && Number.isSafeInteger(file.size) && file.size > 0);
      if (matches.length !== 1) return undefined;
      const file = matches[0];
      return {
        version: label,
        tag,
        checksumsUrl: assetDownloadUrl(tag, "SHA256SUMS"),
        name,
        url: assetDownloadUrl(tag, name),
        bytes: file.size as number,
        ...(typeof file.digest === "string" && /^sha256:[a-f0-9]{64}$/.test(file.digest)
          ? { sha256: file.digest.slice(7) } : {}),
      };
    };
    const dmg = asset(`VniDrop-${label}-arm64.dmg`);
    const deb = asset(`vnidrop_${label}_amd64.deb`);
    const rpm = asset(`vnidrop-${label}.x86_64.rpm`);
    const apk = asset(`VniDrop-${label}.apk`);
    const windowsExe = asset(`VniDrop-${label}-x64.exe`);
    const checksums = asset("SHA256SUMS");
    const manifest = asset("preview-manifest.json");
    if (!checksums || !manifest || (!dmg && !deb && !rpm && !apk && !windowsExe)) continue;
    if (Boolean(deb) !== Boolean(rpm)) continue;
    if (assets.some((file) => assets.filter((other) => other.name === file.name).length !== 1)) continue;

    previews.push({
      version, number: Number(number), channel: "preview", tag,
      tagUrl: `${githubRepoUrl}/releases/tag/${encodeURIComponent(tag)}`,
      publishedAt: release.published_at,
      checksumsUrl: checksums.url,
      manifestUrl: manifest.url,
      dmg, deb, rpm, apk, windowsExe,
    });
  }
  const ordered = previews.sort((a, b) => Date.parse(b.publishedAt) - Date.parse(a.publishedAt) || b.number - a.number);
  if (!ordered.length) return null;
  const latest = { ...ordered[0] };
  for (const release of ordered.slice(1)) {
    latest.dmg ??= release.dmg;
    latest.windowsExe ??= release.windowsExe;
    latest.deb ??= release.deb;
    latest.rpm ??= release.rpm;
    latest.apk ??= release.apk;
  }
  return latest;
}

export async function loadLatestPreview(signal?: AbortSignal): Promise<PreviewRelease | null> {
  // The site is statically exported; the public API keeps previews fresh without a Pages build.
  // Asset metadata also avoids a cross-origin fetch of GitHub's release-download redirects.
  const response = await fetch(`https://api.github.com/repos/${GITHUB_REPO}/releases?per_page=100`, {
    headers: { Accept: "application/vnd.github+json" },
    cache: "no-cache",
    signal,
  });
  if (!response.ok) throw new Error(`Failed to load GitHub previews (${response.status})`);
  return selectLatestPreview(await response.json());
}
