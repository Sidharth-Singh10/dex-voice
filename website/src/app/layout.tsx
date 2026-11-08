import type { Metadata, Viewport } from "next";
import localFont from "next/font/local";
import { Geist_Mono } from "next/font/google";
import "./globals.css";

const cabinet = localFont({
  src: [
    {
      path: "./fonts/cabinet-grotesk-400.woff2",
      weight: "400",
      style: "normal",
    },
    {
      path: "./fonts/cabinet-grotesk-500.woff2",
      weight: "500",
      style: "normal",
    },
    {
      path: "./fonts/cabinet-grotesk-700.woff2",
      weight: "700",
      style: "normal",
    },
    {
      path: "./fonts/cabinet-grotesk-800.woff2",
      weight: "800",
      style: "normal",
    },
  ],
  variable: "--font-cabinet",
  display: "swap",
  fallback: ["ui-sans-serif", "system-ui", "sans-serif"],
});

const geistMono = Geist_Mono({
  subsets: ["latin"],
  variable: "--font-geist-mono",
  display: "swap",
});

const description =
  "Dex Voice is a Linux-first experimental local voice changer built on Parakeet, Phonon, PipeWire, and Rune. Read the signal path, voices, and the limits that still apply.";

export const metadata: Metadata = {
  title: {
    default: "Dex Voice | A local voice changer for Linux",
    template: "%s | Dex Voice",
  },
  description,
  applicationName: "Dex Voice",
  keywords: [
    "Dex Voice",
    "local voice changer",
    "Linux voice changer",
    "Parakeet",
    "Phonon",
    "PipeWire",
    "Vulkan",
  ],
  authors: [{ name: "Dex Voice" }],
  creator: "Dex Voice",
  openGraph: {
    type: "website",
    title: "Dex Voice | A local voice changer for Linux",
    description,
    siteName: "Dex Voice",
  },
  twitter: {
    card: "summary",
    title: "Dex Voice | A local voice changer for Linux",
    description,
  },
  robots: {
    index: true,
    follow: true,
  },
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  viewportFit: "cover",
  colorScheme: "dark",
  themeColor: "#12100e",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" className={`${cabinet.variable} ${geistMono.variable}`}>
      <body className="page-grain bg-ink text-bone antialiased">
        <a className="skip-link" href="#main">
          Skip to content
        </a>
        {children}
      </body>
    </html>
  );
}
