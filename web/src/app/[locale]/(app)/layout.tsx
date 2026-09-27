import { Nav } from '@/components/Nav';
import { WalletProvider } from '@/components/app/Wallet';

/** App screens: daylight surfaces (readable outdoors), sign-in loaded here only. */
export default function AppLayout({ children }: { children: React.ReactNode }) {
  return (
    <WalletProvider>
      <Nav tone="day" />
      <main id="main" tabIndex={-1} className="outline-none min-h-[calc(100dvh-4.5rem)] bg-haze pb-24 text-ink">{children}</main>
    </WalletProvider>
  );
}
