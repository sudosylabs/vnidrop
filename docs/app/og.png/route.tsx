import { ImageResponse } from "next/og";
import { readFile } from "node:fs/promises";
import { join } from "node:path";

export const dynamic = "force-static";

const imageSize = {
  width: 1200,
  height: 630,
};

function BrandMark() {
  return (
    <svg width="48" height="48" viewBox="0 0 1024 1024" fill="none">
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M236.68 148H338.36C366.24 148 387.56 170.96 387.56 198.84V564.56C387.56 597.36 372.8 620.32 372.8 646.56C372.8 725.28 436.76 787.6 522.04 787.6C607.32 787.6 668 725.28 668 646.56C668 620.32 656.52 597.36 656.52 564.56V198.84C656.52 170.96 677.84 148 705.72 148H781.16C817.24 148 846.76 177.52 846.76 213.6V564.56C846.76 738.4 704.08 879.44 522.04 879.44C340 879.44 194.04 738.4 194.04 564.56V374.32H220.28V305.44C195.68 305.44 176 297.24 176 280.84V246.4C176 231.64 187.48 220.16 202.24 220.16H236.68V148ZM256.36 239.84C251.44 239.84 248.16 244.76 248.16 249.68V275.92C248.16 282.48 253.08 285.76 259.64 285.76H282.6C289.16 285.76 292.44 280.84 292.44 274.28V251.32C248.16 244.76 287.52 239.84 280.96 239.84H256.36Z"
        fill="url(#og-brand)"
      />
      <path
        d="M520.4 431.72C495.8 464.52 443.32 530.12 420.36 577.68C390.84 636.72 403.96 699.04 446.6 731.84C487.6 758.08 549.92 758.08 592.56 730.2C633.56 700.68 646.68 636.72 620.44 577.68C597.48 530.12 546.64 464.52 520.4 431.72Z"
        fill="url(#og-drop)"
      />
      <defs>
        <linearGradient id="og-brand" x1="176" y1="148" x2="905" y2="816">
          <stop stopColor="#A855F7" />
          <stop offset="0.48" stopColor="#9D4DF4" />
          <stop offset="1" stopColor="#7C2AEF" />
        </linearGradient>
        <linearGradient id="og-drop" x1="404" y1="432" x2="707" y2="649">
          <stop stopColor="#A855F7" />
          <stop offset="0.48" stopColor="#9D4DF4" />
          <stop offset="1" stopColor="#7C2AEF" />
        </linearGradient>
      </defs>
    </svg>
  );
}

export async function GET() {
  const fontsDir = join(process.cwd(), "og-fonts");
  const [regular, semibold] = await Promise.all([
    readFile(join(fontsDir, "source-sans-400.ttf")),
    readFile(join(fontsDir, "source-sans-600.ttf")),
  ]);

  return new ImageResponse(
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        justifyContent: "space-between",
        padding: "72px 80px 64px",
        background: "#000000",
        color: "#f4f4f5",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <div style={{ display: "flex", alignItems: "center" }}>
          <BrandMark />
          <div
            style={{
              marginLeft: 12,
              fontFamily: "Source Sans 3",
              fontSize: 28,
              fontWeight: 600,
              letterSpacing: -0.6,
            }}
          >
            VniDrop
          </div>
        </div>
        <div
          style={{
            color: "#a1a1aa",
            fontFamily: "Source Sans 3",
            fontSize: 18,
            fontWeight: 400,
          }}
        >
          Open source · Beta
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", maxWidth: 920 }}>
        <div
          style={{
            fontFamily: "Source Sans 3",
            fontSize: 54,
            fontWeight: 600,
            lineHeight: 1.1,
            letterSpacing: -1.4,
          }}
        >
          Send files from this device to that one.
        </div>
        <div
          style={{
            marginTop: 14,
            fontFamily: "Source Sans 3",
            fontSize: 32,
            fontWeight: 400,
            lineHeight: 1.25,
            letterSpacing: -0.5,
            color: "#a1a1aa",
          }}
        >
          They ask. You decide.
        </div>
      </div>

      <div
        style={{
          display: "flex",
          color: "#71717a",
          fontFamily: "Source Sans 3",
          fontSize: 20,
        }}
      >
        Android, iOS, macOS, Windows, and Linux.
      </div>
    </div>,
    {
      ...imageSize,
      fonts: [
        { name: "Source Sans 3", data: regular, weight: 400, style: "normal" },
        { name: "Source Sans 3", data: semibold, weight: 600, style: "normal" },
      ],
    },
  );
}
