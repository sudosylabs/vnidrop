const GITHUB_REPO = "sudosylabs/vnidrop";

export const githubLatestUrl = `https://github.com/${GITHUB_REPO}/releases/latest`;
export const githubRepoUrl = `https://github.com/${GITHUB_REPO}`;
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
};

export type ReleaseAsset = {
  name: string;
  url: string;
  bytes: number;
  sha256: string;
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
};

export function assetDownloadUrl(tag: string, name: string): string {
  return `https://github.com/${GITHUB_REPO}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(name)}`;
}

function toAsset(tag: string, file: ManifestFile): ReleaseAsset {
  return {
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
  const files = manifest.files ?? [];
  const tag = manifest.tag;

  return {
    version: manifest.productVersion,
    channel: manifest.releaseChannel,
    tag,
    tagUrl: `https://github.com/${GITHUB_REPO}/releases/tag/${encodeURIComponent(tag)}`,
    checksumsUrl: assetDownloadUrl(tag, "SHA256SUMS"),
    dmg: findFile(tag, files, /^VniDrop-.+\.dmg$/),
    deb: findFile(tag, files, /\.deb$/),
    rpm: findFile(tag, files, /\.rpm$/),
    apk: findFile(tag, files, /play-universal\.apk$/),
  };
}
