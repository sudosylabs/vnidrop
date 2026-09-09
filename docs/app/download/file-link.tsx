import { formatBytes, type ReleaseAsset } from "@/lib/release";
import styles from "./page.module.css";

export function FileLink({ asset }: { asset: ReleaseAsset }) {
  return (
    <a className={`file-link ${styles.fileLink}`} href={asset.url} rel="noreferrer">
      <span>{asset.name}</span>
      <span className={styles.downloadMeta}>{formatBytes(asset.bytes)}</span>
    </a>
  );
}
