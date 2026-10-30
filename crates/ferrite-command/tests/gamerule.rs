use ferrite_command::gamerule::{GameRules, RuleValue};

#[test]
fn validates_and_roundtrips_rules() {
    let mut rules = GameRules::default();
    assert_eq!(rules.get("randomTickSpeed"), Some(RuleValue::Integer(3)));
    assert!(rules.set("randomTickSpeed", "-1").is_err());
    assert!(rules.set("doDaylightCycle", "1").is_err());
    rules.set("randomTickSpeed", "0").unwrap();
    rules.set("doDaylightCycle", "false").unwrap();
    let json = serde_json::to_string(&rules).unwrap();
    let restored: GameRules = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.get("randomTickSpeed"), Some(RuleValue::Integer(0)));
    assert_eq!(
        restored.get("doDaylightCycle"),
        Some(RuleValue::Boolean(false))
    );
}
