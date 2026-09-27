import { NextResponse, type NextRequest } from 'next/server';

const LOCALES = ['en', 'fil'];

/** Sends a path without a language to the reader's: Filipino (or Tagalog) readers get Filipino. */
export function proxy(request: NextRequest) {
  const { pathname } = request.nextUrl;
  if (LOCALES.some((l) => pathname === `/${l}` || pathname.startsWith(`/${l}/`))) {
    // The visitor's country, for the landing's first guess (the page itself stays static). Only an
    // ISO code is stored, and only on this site.
    const cc = request.headers.get('x-vercel-ip-country');
    if (!cc || request.cookies.get('cc')?.value === cc) return;
    const res = NextResponse.next();
    res.cookies.set('cc', cc.slice(0, 2), { path: '/', maxAge: 60 * 60 * 24 * 30, sameSite: 'lax', secure: true });
    return res;
  }
  const accept = request.headers.get('accept-language') ?? '';
  const locale = /\b(fil|tl)\b/i.test(accept) ? 'fil' : 'en';
  request.nextUrl.pathname = `/${locale}${pathname === '/' ? '' : pathname}`;
  return NextResponse.redirect(request.nextUrl);
}

export const config = {
  // Everything except Next internals, API routes and files with an extension.
  matcher: ['/((?!_next|api|.*\\..*).*)'],
};
