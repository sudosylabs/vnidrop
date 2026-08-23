import path from "node:path";
import { fileURLToPath } from "node:url";
import type { NextConfig } from "next";

const docsRoot = path.dirname(fileURLToPath(import.meta.url));

const nextConfig: NextConfig = {
  output: "export",
  trailingSlash: true,
  turbopack: {
    root: docsRoot,
  },
};

export default nextConfig;
