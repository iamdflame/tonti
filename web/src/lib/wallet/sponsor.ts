// Kept apart from senders.ts so a page can ask "is gas sponsored?" without loading any wallet code.
export const alchemy = { key: process.env.NEXT_PUBLIC_ALCHEMY_API_KEY, policy: process.env.NEXT_PUBLIC_ALCHEMY_POLICY_ID };
/** One-tap, gasless transactions need an Alchemy key and a Gas Manager policy for Robinhood Chain. */
export const canSponsor = Boolean(alchemy.key && alchemy.policy);
