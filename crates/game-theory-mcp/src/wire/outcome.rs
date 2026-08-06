//! Tool output envelope.
//!
//! Domain failures are *results*, not JSON-RPC errors: a JSON-RPC error is
//! addressed to the client, and some hosts surface only its `message`, which
//! would drop the diagnostics that make `InvalidGame` actionable. Every
//! `GtError` describes something the host LLM can fix, so every one lands here.
//!
//! `SyncTool::invoke` returns `Result<Output, Error>` where `Error: Into<ErrorData>`
//! -- that channel *is* the protocol-error path. So domain errors live inside
//! `Output`, and `Error` carries only malformed-request cases.

use game_theory_core::error::{Diagnostic, GtError};
use rmcp::model::{CallToolResult, JsonObject};
use rmcp::ErrorData;
use schemars::JsonSchema;
use serde::Serialize;
use std::sync::Arc;

/// Either a tool's payload or a domain failure. The discriminant `ok` is a
/// boolean, which rules out serde's internally-tagged representation --
/// `#[serde(tag = "ok")]` writes the variant *name*, yielding `{"ok":"true"}`.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ToolOutput<T: Serialize + JsonSchema> {
    Ok(OkEnvelope<T>),
    Err(ErrEnvelope),
}

impl<T: Serialize + JsonSchema> ToolOutput<T> {
    pub fn ok(result: T) -> Self {
        ToolOutput::Ok(OkEnvelope { ok: true, result })
    }

    /// Render as a `CallToolResult`, setting `isError` on the failure arm.
    ///
    /// Tools return `CallToolResult` rather than a plain payload because
    /// rmcp's `sync_tool_wrapper` routes `Ok(output)` through
    /// `CallToolResult::success`, which hard-codes `isError: false`. Its only
    /// `isError: true` path runs through the `Err` channel, and that channel
    /// is typed `Into<ErrorData>` -- a JSON-RPC error, which is precisely what
    /// a domain failure must not become. Building the result here is what
    /// keeps `isError` and the `ok` discriminant agreeing.
    pub fn into_call_tool_result(self) -> CallToolResult {
        let failed = matches!(self, ToolOutput::Err(_));
        let value = serde_json::to_value(&self).unwrap_or(serde_json::Value::Null);
        if failed {
            CallToolResult::structured_error(value)
        } else {
            CallToolResult::structured(value)
        }
    }
}

/// Build a tool's output schema from the payload type.
///
/// Tools declare `Output = CallToolResult` so they can set `isError`, but
/// `CallToolResult`'s own schema is useless to a host. Each tool overrides
/// `ToolBase::output_schema` with this, so the published schema describes
/// what the tool actually returns.
pub fn output_schema_of<T: JsonSchema>() -> Option<Arc<JsonObject>> {
    let schema = schemars::SchemaGenerator::default().root_schema_for::<T>();
    match serde_json::to_value(schema).ok()? {
        serde_json::Value::Object(map) => Some(Arc::new(map)),
        _ => None,
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct OkEnvelope<T: Serialize + JsonSchema> {
    /// Always `true`.
    pub ok: bool,
    #[serde(flatten)]
    pub result: T,
}

/// A domain failure the host LLM can act on.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ErrEnvelope {
    /// Always `false`.
    pub ok: bool,
    /// Stable machine-readable code.
    pub code: &'static str,
    /// One-sentence description of what went wrong.
    pub message: String,
    /// What to do instead. Never empty.
    pub suggestion: String,
    #[serde(flatten)]
    pub detail: ErrorDetail,
}

/// Per-variant fields, flattened into the envelope.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ErrorDetail {
    None {},
    Diagnostics {
        diagnostics: Vec<WireDiagnostic>,
    },
    TooLarge {
        field: &'static str,
        limit: usize,
        actual: usize,
    },
    Tool {
        tool: &'static str,
    },
    Players {
        players: usize,
    },
    InformationSet {
        information_set: usize,
    },
    Form {
        expected: &'static str,
        actual: &'static str,
    },
    Profile {
        player: usize,
        profile: Vec<usize>,
    },
    PlayerReason {
        player: usize,
        reason: String,
    },
    Value {
        value: String,
    },
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WireDiagnostic {
    pub code: String,
    pub message: String,
}

impl From<&Diagnostic> for WireDiagnostic {
    fn from(d: &Diagnostic) -> Self {
        // DiagnosticCode derives Debug but not Serialize; the Debug name is
        // the stable identifier, lowercased to snake_case for the wire.
        WireDiagnostic {
            code: to_snake_case(&format!("{:?}", d.code)),
            message: d.message.clone(),
        }
    }
}

fn to_snake_case(camel: &str) -> String {
    let mut out = String::with_capacity(camel.len() + 4);
    for (i, ch) in camel.char_indices() {
        if ch.is_uppercase() && i != 0 {
            out.push('_');
        }
        out.extend(ch.to_lowercase());
    }
    out
}

impl From<GtError> for ErrEnvelope {
    fn from(e: GtError) -> Self {
        let message = e.to_string();
        let (code, suggestion, detail) = match &e {
            GtError::InvalidGame { diagnostics } => (
                "invalid_game",
                "fix each problem listed in `diagnostics`, then call validate_game again"
                    .to_string(),
                ErrorDetail::Diagnostics {
                    diagnostics: diagnostics.iter().map(WireDiagnostic::from).collect(),
                },
            ),
            GtError::GameTooLarge {
                limit,
                actual,
                field,
            } => (
                "game_too_large",
                format!("reduce `{field}` to at most {limit}; this game has {actual}"),
                ErrorDetail::TooLarge {
                    field,
                    limit: *limit,
                    actual: *actual,
                },
            ),
            GtError::OrdinalPayoffsRejected { tool } => (
                "ordinal_payoffs_rejected",
                format!(
                    "{tool} takes expectations, which are meaningless over ranks; \
                     supply cardinal utilities and set payoff_kind to \"cardinal\""
                ),
                ErrorDetail::Tool { tool },
            ),
            GtError::NPlayerMixedUnsupported { players } => (
                "n_player_mixed_unsupported",
                "n-player mixed Nash is PPAD-complete and any implementation would be \
                 approximate; call solve_pure_nash instead"
                    .to_string(),
                ErrorDetail::Players { players: *players },
            ),
            GtError::ImperfectInformationUnsupported { information_set } => (
                "imperfect_information_unsupported",
                "v1.0 tree solvers require singleton information sets; split the \
                 non-singleton set, or model the game in strategic form"
                    .to_string(),
                ErrorDetail::InformationSet {
                    information_set: *information_set,
                },
            ),
            GtError::WrongGameForm { expected, actual } => (
                "wrong_game_form",
                format!(
                    "this call needs a {expected} game but got a {actual} one; \
                     use convert_form, or send the game in {expected} form"
                ),
                ErrorDetail::Form { expected, actual },
            ),
            GtError::UnknownProfile { player, profile } => (
                "unknown_profile",
                "every strategy profile needs exactly one outcome; add the missing \
                 profile to `outcomes`"
                    .to_string(),
                ErrorDetail::Profile {
                    player: *player,
                    profile: profile.clone(),
                },
            ),
            GtError::InvalidMixedStrategy { player, reason } => (
                "invalid_mixed_strategy",
                "each player's mixture needs one probability per strategy, each at \
                 least zero, summing to exactly 1"
                    .to_string(),
                ErrorDetail::PlayerReason {
                    player: *player,
                    reason: reason.clone(),
                },
            ),
            GtError::InvalidPlanProfile { player, reason } => (
                "invalid_plan_profile",
                "a plan fixes an action at every decision node of its player, \
                 including nodes unreachable given that player's own earlier choices"
                    .to_string(),
                ErrorDetail::PlayerReason {
                    player: *player,
                    reason: reason.clone(),
                },
            ),
            GtError::NoPureNashForPunishment => (
                "no_pure_nash_for_punishment",
                "grim trigger reverts to a pure Nash equilibrium of the stage game, \
                 and this stage game has none; choose a stage game that has one"
                    .to_string(),
                ErrorDetail::None {},
            ),
            GtError::InvalidDiscountFactor { value } => (
                "invalid_discount_factor",
                "the discount factor must lie in [0, 1); supply a fraction like \"1/3\""
                    .to_string(),
                ErrorDetail::Value {
                    value: value.clone(),
                },
            ),
        };
        ErrEnvelope {
            ok: false,
            code,
            message,
            suggestion,
            detail,
        }
    }
}

impl<T: Serialize + JsonSchema> From<GtError> for ToolOutput<T> {
    fn from(e: GtError) -> Self {
        ToolOutput::Err(e.into())
    }
}

/// A malformed request the host cannot fix by editing the game -- this is the
/// only thing that becomes a JSON-RPC error.
#[derive(Debug)]
pub enum RequestError {
    /// A field was present but unusable, e.g. a probability that is not a fraction.
    InvalidParams(String),
}

impl From<RequestError> for ErrorData {
    fn from(e: RequestError) -> Self {
        match e {
            RequestError::InvalidParams(m) => ErrorData::invalid_params(m, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_theory_core::error::{Diagnostic, DiagnosticCode};
    use serde::Serialize;

    #[derive(Debug, Serialize, schemars::JsonSchema)]
    struct Payload {
        holds: bool,
    }

    #[test]
    fn ok_serializes_with_a_boolean_discriminant_and_flattened_payload() {
        let out = ToolOutput::ok(Payload { holds: true });
        let v = serde_json::to_value(&out).unwrap();
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        assert_eq!(v["holds"], serde_json::Value::Bool(true));
    }

    #[test]
    fn err_serializes_with_a_false_discriminant() {
        let out: ToolOutput<Payload> = GtError::NoPureNashForPunishment.into();
        let v = serde_json::to_value(&out).unwrap();
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "no_pure_nash_for_punishment");
    }

    #[test]
    fn every_domain_error_carries_a_nonempty_suggestion() {
        let errors = vec![
            GtError::InvalidGame {
                diagnostics: vec![Diagnostic {
                    code: DiagnosticCode::NoPlayers,
                    message: "no players".into(),
                }],
            },
            GtError::GameTooLarge {
                limit: 32,
                actual: 40,
                field: "strategies",
            },
            GtError::OrdinalPayoffsRejected {
                tool: "verify_mixed_nash",
            },
            GtError::NPlayerMixedUnsupported { players: 3 },
            GtError::ImperfectInformationUnsupported { information_set: 1 },
            GtError::WrongGameForm {
                expected: "strategic",
                actual: "extensive",
            },
            GtError::UnknownProfile {
                player: 0,
                profile: vec![0, 1],
            },
            GtError::InvalidMixedStrategy {
                player: 0,
                reason: "sums to 3/4".into(),
            },
            GtError::InvalidPlanProfile {
                player: 1,
                reason: "node 1 is unset".into(),
            },
            GtError::NoPureNashForPunishment,
            GtError::InvalidDiscountFactor {
                value: "3/2".into(),
            },
        ];
        for e in errors {
            let env = ErrEnvelope::from(e);
            assert!(!env.suggestion.is_empty(), "{} has no suggestion", env.code);
            assert!(!env.message.is_empty(), "{} has no message", env.code);
            assert!(!env.ok, "{} must carry ok:false", env.code);
        }
    }

    #[test]
    fn invalid_game_carries_every_diagnostic() {
        let e = GtError::InvalidGame {
            diagnostics: vec![
                Diagnostic {
                    code: DiagnosticCode::NoPlayers,
                    message: "a".into(),
                },
                Diagnostic {
                    code: DiagnosticCode::MissingProfile,
                    message: "b".into(),
                },
            ],
        };
        let v = serde_json::to_value(ErrEnvelope::from(e)).unwrap();
        assert_eq!(v["code"], "invalid_game");
        assert_eq!(v["diagnostics"].as_array().unwrap().len(), 2);
        assert_eq!(v["diagnostics"][0]["code"], "no_players");
    }

    #[test]
    fn game_too_large_carries_its_numbers() {
        let e = GtError::GameTooLarge {
            limit: 32,
            actual: 40,
            field: "strategies",
        };
        let v = serde_json::to_value(ErrEnvelope::from(e)).unwrap();
        assert_eq!(v["code"], "game_too_large");
        assert_eq!(v["limit"], 32);
        assert_eq!(v["actual"], 40);
        assert_eq!(v["field"], "strategies");
    }

    #[test]
    fn the_ok_arm_renders_a_result_with_is_error_false() {
        let r = ToolOutput::ok(Payload { holds: true }).into_call_tool_result();
        assert_eq!(r.is_error, Some(false));
        assert_eq!(
            r.structured_content.as_ref().unwrap()["ok"],
            serde_json::Value::Bool(true)
        );
    }

    #[test]
    fn the_err_arm_renders_a_result_with_is_error_true() {
        let out: ToolOutput<Payload> = GtError::NoPureNashForPunishment.into();
        let r = out.into_call_tool_result();
        // isError and the ok discriminant must agree.
        assert_eq!(r.is_error, Some(true));
        let sc = r.structured_content.as_ref().unwrap();
        assert_eq!(sc["ok"], serde_json::Value::Bool(false));
        assert_eq!(sc["code"], "no_pure_nash_for_punishment");
    }

    #[test]
    fn the_output_schema_describes_the_payload_not_call_tool_result() {
        let schema = output_schema_of::<ToolOutput<Payload>>().unwrap();
        let text = serde_json::to_string(&*schema).unwrap();
        assert!(
            text.contains("holds"),
            "schema should describe the payload: {text}"
        );
    }

    #[test]
    fn wrong_game_form_names_convert_form() {
        let e = GtError::WrongGameForm {
            expected: "extensive",
            actual: "strategic",
        };
        let env = ErrEnvelope::from(e);
        assert!(
            env.suggestion.contains("convert_form"),
            "got {}",
            env.suggestion
        );
    }

    #[test]
    fn n_player_mixed_names_solve_pure_nash() {
        let e = GtError::NPlayerMixedUnsupported { players: 3 };
        let env = ErrEnvelope::from(e);
        assert!(
            env.suggestion.contains("solve_pure_nash"),
            "got {}",
            env.suggestion
        );
    }
}
