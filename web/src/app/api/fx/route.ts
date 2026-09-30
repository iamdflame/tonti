// USD against the currencies of the countries Tonti prices, from the ECB's daily reference rates
// (frankfurter.dev, no key; frankfurter.app now redirects there). Countries whose currency the ECB doesn't publish show dollars only.
const SYMBOLS = ['PHP', 'IDR', 'INR', 'MYR', 'THB', 'CNY', 'SGD'];

export async function GET() {
  try {
    const r = await fetch(`https://api.frankfurter.dev/v1/latest?from=USD&to=${SYMBOLS.join(',')}`, { next: { revalidate: 43_200 }, signal: AbortSignal.timeout(6_000) });
    if (!r.ok) return Response.json({ error: 'rates unavailable' }, { status: 502 });
    const d = (await r.json()) as { date: string; rates: Record<string, number> };
    return Response.json({ date: d.date, rates: d.rates, source: 'ECB via frankfurter.dev' }, { headers: { 'Cache-Control': 'public, s-maxage=43200, stale-while-revalidate=86400' } });
  } catch {
    return Response.json({ error: 'rates unavailable' }, { status: 502 });
  }
}
