export default {
  'scripts/**/*.ts': [
    'oxfmt --write --no-error-on-unmatched-pattern',
    'oxlint --fix --deny-warnings --no-error-on-unmatched-pattern',
  ],
  '.ox{fmt,lint}rc.json': 'oxfmt --write --no-error-on-unmatched-pattern',
  '{package,scripts/tsconfig}.json': 'oxfmt --write --no-error-on-unmatched-pattern',
  '{crates,src-iced}/**/*.rs': () => 'bun run task rust fmt',
};
