import { countries } from '@/sdk/countries.ts';

/** Countries the Actuary prices. */
export const countryCount = countries.length;

/** Every cohort the Actuary prices: countries × both sexes × the fitted birth years. */
export const cohortCount = countries.reduce((n, c) => n + 2 * (c.birthYears[1] - c.birthYears[0] + 1), 0);
