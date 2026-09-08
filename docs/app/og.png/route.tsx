/* eslint-disable @next/next/no-img-element -- ImageResponse embeds local assets into the exported PNG. */
import { ImageResponse } from "next/og";
import { readFile } from "node:fs/promises";
import { join } from "node:path";

export const dynamic = "force-static";

export async function GET() {
  const root = process.cwd();
  const [regular, semibold, brand, screenshot] = await Promise.all([
    readFile(join(root, "og-fonts/source-sans-400.ttf")),
    readFile(join(root, "og-fonts/source-sans-600.ttf")),
    readFile(join(root, "public/brand-mark.svg")),
    readFile(join(root, "public/shots/hero.png")),
  ]);

  return new ImageResponse(
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        background: "#050506",
        color: "#f3f1f5",
        fontFamily: "Source Sans 3",
      }}
    >
      <div
        style={{
          position: "absolute",
          left: 64,
          top: 64,
          bottom: 54,
          width: 380,
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <img
            src={`data:image/svg+xml;base64,${brand.toString("base64")}`}
            width={64}
            height={64}
            alt=""
          />
          <div style={{ fontSize: 52, fontWeight: 600, letterSpacing: -1.5 }}>
            VniDrop
          </div>
        </div>

        <div style={{ display: "flex", flexDirection: "column" }}>
          <div style={{ fontSize: 38, fontWeight: 400, lineHeight: 1.2, letterSpacing: -0.5 }}>
            File sharing across phones and computers.
          </div>
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              marginTop: 28,
              color: "#aaa6b0",
              fontSize: 21,
              lineHeight: 1.5,
            }}
          >
            <div>Android · iOS · macOS</div>
            <div>Windows · Linux</div>
          </div>
        </div>

        <div style={{ color: "#aaa6b0", fontSize: 19 }}>
          vnidrop.sudosy.fr
        </div>
      </div>

      <img
        src={`data:image/png;base64,${screenshot.toString("base64")}`}
        width={745}
        height={596}
        alt="VniDrop running on macOS and iPhone"
        style={{ position: "absolute", left: 455, top: 24 }}
      />
    </div>,
    {
      width: 1200,
      height: 630,
      fonts: [
        { name: "Source Sans 3", data: regular, weight: 400, style: "normal" },
        { name: "Source Sans 3", data: semibold, weight: 600, style: "normal" },
      ],
    },
  );
}
