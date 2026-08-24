import Link from "next/link";
import Image from "next/image";
import styles from "./brand.module.css";

type BrandAssetProps = {
  className?: string;
  title?: string;
};

export function BrandMark({ className, title }: BrandAssetProps) {
  return (
    <Image
      className={className}
      src="/brand-mark.svg"
      width={1024}
      height={1024}
      alt={title ?? ""}
      aria-hidden={title ? undefined : true}
      unoptimized
    />
  );
}

export function Brand() {
  return (
    <Link className={styles.brand} href="/" aria-label="VniDrop home">
      <BrandMark className={styles.mark} />
      <span className={styles.name} translate="no">
        VniDrop
      </span>
    </Link>
  );
}
