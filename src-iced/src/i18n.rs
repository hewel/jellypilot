//! Embedded application-owned text. Domain data and technical diagnostics are not translated.

pub(crate) mod media;

use std::borrow::Cow;
use std::collections::HashMap;

pub(crate) use fluent_templates::fluent_bundle::FluentValue;
use fluent_templates::{langid, static_loader, LanguageIdentifier};
use jellypilot_core::locale::{resolve_language, LanguagePreference, UiLanguage};

static ENGLISH: LanguageIdentifier = langid!("en-US");
static CHINESE: LanguageIdentifier = langid!("zh-CN");

static_loader! {
  static MESSAGES = {
    locales: "./locales",
    fallback_language: "en-US",
  };
}

/// A language-independent message retained across asynchronous work and live switches.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UiText {
  id: &'static str,
  args: Vec<(&'static str, FluentValue<'static>)>,
}

impl UiText {
  pub const fn new(id: &'static str) -> Self {
    Self {
      id,
      args: Vec::new(),
    }
  }

  pub const fn id(&self) -> &'static str {
    self.id
  }

  pub fn arg(mut self, name: &'static str, value: impl Into<FluentValue<'static>>) -> Self {
    self.args.push((name, value.into()));
    self
  }
}

/// Cheap, explicit view context; the embedded resources never hold a mutable language.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Localizer {
  language: UiLanguage,
}

impl Default for Localizer {
  fn default() -> Self {
    Self::new(UiLanguage::English)
  }
}

impl Localizer {
  pub const fn new(language: UiLanguage) -> Self {
    Self { language }
  }

  /// Query OS preference only at startup or an explicit preference selection.
  pub fn resolve(preference: LanguagePreference) -> Self {
    let language = match preference {
      LanguagePreference::System => resolve_language(preference, sys_locale::get_locales()),
      LanguagePreference::Fixed(language) => language,
    };
    Self::new(language)
  }

  fn identifier(self) -> &'static LanguageIdentifier {
    match self.language {
      UiLanguage::English => &ENGLISH,
      UiLanguage::SimplifiedChinese => &CHINESE,
    }
  }

  pub fn text(self, id: &str) -> String {
    self.lookup(id, None)
  }

  pub fn format(self, id: &str, args: &[(&str, FluentValue<'_>)]) -> String {
    if args.is_empty() {
      return self.text(id);
    }
    let args = args
      .iter()
      .map(|(name, value)| {
        // Formatting a retained message must not copy its owned argument strings.
        let value = match value {
          FluentValue::String(value) => FluentValue::String(Cow::Borrowed(value.as_ref())),
          value => value.clone(),
        };
        (*name, value)
      })
      .collect();
    self.lookup(id, Some(&args))
  }

  pub fn message(self, message: &UiText) -> String {
    self.format(message.id, &message.args)
  }

  fn lookup(self, id: &str, args: Option<&HashMap<&str, FluentValue<'_>>>) -> String {
    // Resolve exactly first: Fluent's language-range negotiation otherwise lets
    // a Traditional-Chinese request match the Simplified-Chinese resource.
    match MESSAGES.lookup_single_language(self.identifier(), id, args) {
      Ok(text) => return text,
      Err(error) if self.language != UiLanguage::English => {
        tracing::warn!(message_id = id, locale = self.language.tag(), error = %error, "UI translation lookup failed; trying English");
        match MESSAGES.lookup_single_language(&ENGLISH, id, args) {
          Ok(text) => return text,
          Err(error) => {
            tracing::error!(message_id = id, error = %error, "English UI fallback failed")
          }
        }
      }
      Err(error) => {
        tracing::error!(message_id = id, error = %error, "English UI translation lookup failed")
      }
    }
    MESSAGES
      .lookup_single_language::<&str>(self.identifier(), "common-unavailable", None)
      .expect("the embedded base messages are validated")
  }

  /// Reading-oriented duration, distinct from stable MPV timecodes and protocol units.
  pub fn duration(self, seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
      return "—".to_owned();
    }
    let minutes = (seconds / 60.0).round().max(1.0) as u64;
    let hours = minutes / 60;
    let minutes = minutes % 60;
    if hours == 0 {
      self.format("duration-minutes", &[("count", minutes.into())])
    } else if minutes == 0 {
      self.format("duration-hours", &[("count", hours.into())])
    } else {
      self.format(
        "duration-hours-minutes",
        &[("hours", hours.into()), ("minutes", minutes.into())],
      )
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fluent_syntax::ast::{Expression, InlineExpression, Pattern, PatternElement};
  use std::collections::{BTreeMap, BTreeSet};

  fn pattern_variables(pattern: &Pattern<&str>, variables: &mut BTreeSet<String>) {
    for element in &pattern.elements {
      if let PatternElement::Placeable { expression } = element {
        expression_variables(expression, variables);
      }
    }
  }

  fn expression_variables(expression: &Expression<&str>, variables: &mut BTreeSet<String>) {
    match expression {
      Expression::Inline(inline) => inline_variables(inline, variables),
      Expression::Select { selector, variants } => {
        inline_variables(selector, variables);
        for variant in variants {
          pattern_variables(&variant.value, variables);
        }
      }
    }
  }

  fn inline_variables(inline: &InlineExpression<&str>, variables: &mut BTreeSet<String>) {
    match inline {
      InlineExpression::VariableReference { id } => {
        variables.insert(id.name.to_owned());
      }
      InlineExpression::Placeable { expression } => expression_variables(expression, variables),
      InlineExpression::FunctionReference { arguments, .. }
      | InlineExpression::TermReference {
        arguments: Some(arguments),
        ..
      } => {
        for argument in &arguments.positional {
          inline_variables(argument, variables);
        }
        for argument in &arguments.named {
          inline_variables(&argument.value, variables);
        }
      }
      _ => {}
    }
  }

  fn messages(language: &str) -> BTreeMap<String, BTreeSet<String>> {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("locales")
      .join(language);
    let mut messages = BTreeMap::new();
    for entry in std::fs::read_dir(directory).unwrap() {
      let path = entry.unwrap().path();
      if path.extension().and_then(|extension| extension.to_str()) != Some("ftl") {
        continue;
      }
      let source = std::fs::read_to_string(&path).unwrap();
      let resource = fluent_syntax::parser::parse(source.as_str())
        .unwrap_or_else(|(_, errors)| panic!("{}: {errors:?}", path.display()));
      for entry in resource.body {
        if let fluent_syntax::ast::Entry::Message(message) = entry {
          let mut variables = BTreeSet::new();
          pattern_variables(
            message
              .value
              .as_ref()
              .expect("messages have visible values"),
            &mut variables,
          );
          assert!(
            messages
              .insert(message.id.name.to_owned(), variables)
              .is_none(),
            "duplicate {language} message {}",
            message.id.name
          );
        }
      }
    }
    messages
  }

  #[test]
  fn shipped_messages_preserve_argument_contracts_and_core_chinese_coverage() {
    let english = messages("en-US");
    let chinese = messages("zh-CN");
    let untranslated: Vec<_> = english
      .keys()
      .filter(|id| !chinese.contains_key(*id))
      .collect();
    eprintln!(
      "Untranslated zh-CN messages (only non-core entries may fall back): {untranslated:?}"
    );
    for (id, variables) in &chinese {
      let english_variables = english
        .get(id)
        .unwrap_or_else(|| panic!("missing English base: {id}"));
      assert!(
        variables.is_subset(english_variables),
        "{id}: Chinese requires undeclared arguments"
      );
    }
    for (id, variables) in &english {
      let core = [
        "common-",
        "language-",
        "login-",
        "account-",
        "settings-",
        "player-",
        "tray-",
      ]
      .iter()
      .any(|prefix| id.starts_with(prefix));
      assert!(
        !core || chinese.contains_key(id),
        "missing core Chinese message: {id}"
      );
      for number in [0_u32, 1, 2] {
        let args: HashMap<_, _> = variables
          .iter()
          .map(|name| {
            let value = if name.starts_with("has-") || name == "favorite" {
              if number == 1 { "yes" } else { "no" }.into()
            } else {
              number.into()
            };
            (name.as_str(), value)
          })
          .collect();
        let base = MESSAGES
          .lookup_single_language(&ENGLISH, id, Some(&args))
          .unwrap_or_else(|error| panic!("{ENGLISH}/{id}: {error}"));
        if chinese.contains_key(id) {
          MESSAGES
            .lookup_single_language(&CHINESE, id, Some(&args))
            .unwrap_or_else(|error| panic!("{CHINESE}/{id}: {error}"));
        } else {
          assert_eq!(
            Localizer::new(UiLanguage::SimplifiedChinese).lookup(id, Some(&args)),
            base,
            "{id}: missing non-core Chinese must use English"
          );
        }
      }
    }
  }

  #[test]
  fn invalid_message_arguments_show_localized_feedback_not_placeholders() {
    let locale = Localizer::new(UiLanguage::SimplifiedChinese);
    assert_eq!(
      locale.text("duration-minutes"),
      locale.text("common-unavailable")
    );
  }
}
