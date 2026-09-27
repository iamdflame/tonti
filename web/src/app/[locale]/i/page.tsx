import { Nav } from '@/components/Nav';
import { Invite } from '@/components/app/Invite';

/** The parent's invite: plain words, big buttons, her own phone. No sign-in needed. */
export default function InvitePage() {
  return (
    <>
      <Nav tone="day" />
      <main id="main" tabIndex={-1} className="outline-none min-h-[calc(100dvh-4.5rem)] bg-haze pb-24 text-ink">
        <Invite />
      </main>
    </>
  );
}
