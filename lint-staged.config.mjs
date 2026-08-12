export default {
  '*.config.{js,json,jsonc,mjs,ts}': [
    'oxfmt --write --no-error-on-unmatched-pattern',
    'oxlint --fix --deny-warnings --no-error-on-unmatched-pattern',
  ],
  '.ox{fmt,lint}rc.json': 'oxfmt --write --no-error-on-unmatched-pattern',
  'scripts/**/*.ts': [
    'oxfmt --write --no-error-on-unmatched-pattern',
    'oxlint --fix --deny-warnings --no-error-on-unmatched-pattern',
  ],
  'src-tauri/**/*.json': 'oxfmt --write --no-error-on-unmatched-pattern',
  'src-tauri/**/*.rs': () => 'cargo fmt --manifest-path src-tauri/Cargo.toml',
  'src-gtk/**/*.rs': () => 'cargo fmt --manifest-path src-gtk/Cargo.toml',
  'e2e/**/*.{js,jsx,ts,tsx}': [
    'oxfmt --write --no-error-on-unmatched-pattern',
    'oxlint --fix --deny-warnings --no-error-on-unmatched-pattern',
  ],
  'e2e/**/*.{json,jsonc}': 'oxfmt --write --no-error-on-unmatched-pattern',
  'src/**/*.{css,json,jsonc}': 'oxfmt --write --no-error-on-unmatched-pattern',
  'src/**/*.{js,jsx,ts,tsx}': [
    'oxfmt --write --no-error-on-unmatched-pattern',
    'oxlint --fix --deny-warnings --no-error-on-unmatched-pattern',
  ],
  'tests/**/*.{js,jsx,ts,tsx}': [
    'oxfmt --write --no-error-on-unmatched-pattern',
    'oxlint --fix --deny-warnings --no-error-on-unmatched-pattern',
  ],
  'tests/**/*.{json,jsonc}': 'oxfmt --write --no-error-on-unmatched-pattern',
  '{package,tsconfig}.json': 'oxfmt --write --no-error-on-unmatched-pattern',
};
