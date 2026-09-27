import type { NextConfig } from 'next';

const nextConfig: NextConfig = {
  experimental: {
    // The build machine's disk is nearly full: no persistent Turbopack cache on disk.
    turbopackFileSystemCacheForDev: false,
    turbopackFileSystemCacheForBuild: false,
  },
  async headers() {
    return [
      {
        source: '/(.*)',
        headers: [
          { key: 'X-Content-Type-Options', value: 'nosniff' },
          { key: 'Referrer-Policy', value: 'strict-origin-when-cross-origin' },
          { key: 'Permissions-Policy', value: 'camera=(), microphone=(), geolocation=(), publickey-credentials-get=(self), publickey-credentials-create=(self)' },
        ],
      },
    ];
  },
};

export default nextConfig;
