import { formatBytes, type ReleaseAsset } from "@/lib/release";
import styles from "./page.module.css";

export function FileLink({ asset }: { asset: ReleaseAsset }) {
  return (
    <span className={styles.downloadFile}>
      <a className={`file-link ${styles.fileLink}`} href={asset.url} rel="noreferrer">
        <span>{asset.name}</span>
        <span className={styles.downloadMeta}>{formatBytes(asset.bytes)}</span>
      </a>
      <span className={styles.downloadMeta}>
        {asset.version}{" · "}<a className="text-link" href={asset.checksumsUrl} rel="noreferrer">SHA256 checksums</a>
      </span>
    </span>
  );
}
