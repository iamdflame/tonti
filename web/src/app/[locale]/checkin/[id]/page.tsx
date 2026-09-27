import { Nav } from '@/components/Nav';
import { CheckIn } from '@/components/app/CheckIn';

/** A parent's check-in: one button, every three months. No sign-in, no wallet, no gas. */
export default async function CheckInPage({ params }: PageProps<'/[locale]/checkin/[id]'>) {
  const { id } = await params;
  return (
    <>
      <Nav tone="day" />
      <main id="main" tabIndex={-1} className="outline-none min-h-[calc(100dvh-4.5rem)] bg-haze pb-24 text-ink">
        <CheckIn id={/^\d+$/.test(id) ? BigInt(id) : null} />
      </main>
    </>
  );
}
