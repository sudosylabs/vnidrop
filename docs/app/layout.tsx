import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import "./globals.css";

const configuredSiteUrl = process.env.NEXT_PUBLIC_SITE_URL ?? "https://vnidrop.sudosy.fr";

const metadataBase = new URL(
  configuredSiteUrl.startsWith("http") ? configuredSiteUrl : `https://${configuredSiteUrl}`,
);

const title = "VniDrop — Send files from this device to that one";
const description =
  "Direct file transfer across Android, iOS, macOS, Windows, and Linux. No account, no hosted copy. Meet with an invitation, or send to a Saved device. They still confirm.";

export const metadata: Metadata = {
  metadataBase,
  title: {
    default: title,
    template: "%s · VniDrop",
  },
  description,
  applicationName: "VniDrop",
  manifest: "/site.webmanifest",
  keywords: [
    "peer-to-peer file transfer",
    "encrypted file sharing",
    "cross-platform file transfer",
    "open source",
  ],
  openGraph: {
    type: "website",
    siteName: "VniDrop",
    title,
    description,
    images: [
      {
        url: "/og.png",
        width: 1200,
        height: 630,
        alt: "VniDrop — Send files from this device to that one.",
      },
    ],
  },
  twitter: {
    card: "summary_large_image",
    title,
    description,
    images: [
      {
        url: "/og.png",
        alt: "VniDrop — Send files from this device to that one.",
      },
    ],
  },
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#000000" },
    { media: "(prefers-color-scheme: dark)", color: "#000000" },
  ],
};

export default function RootLayout({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <html lang="en" data-scroll-behavior="smooth">
      <body>
        <a className="skip-link" href="#main-content">
          Skip to content
        </a>
        <SiteHeader />
        {children}
        <SiteFooter />
      </body>
    </html>
  );
}
