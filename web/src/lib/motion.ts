'use client';
import { useEffect, useState } from 'react';

/** prefers-reduced-motion, live. */
export function useReducedMotion() {
  const [reduced, set] = useState(false);
  useEffect(() => {
    const m = window.matchMedia('(prefers-reduced-motion: reduce)');
    set(m.matches);
    const on = () => set(m.matches);
    m.addEventListener('change', on);
    return () => m.removeEventListener('change', on);
  }, []);
  return reduced;
}
