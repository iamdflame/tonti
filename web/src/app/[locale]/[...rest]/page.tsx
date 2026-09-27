import { notFound } from 'next/navigation';

/** Any path under a language that no page answers: the language's own 404 (not-found.tsx beside
 * the layout), so it keeps the page's language, fonts and navigation. */
export default function Rest() {
  notFound();
}
