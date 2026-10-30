//! Feedback text for commands.
//!
//! Feedback carries vanilla's translation keys and arguments; the protocol only has a plain
//! text component so the English 26.3 `en_us` strings are rendered here. Keeping the keyed
//! lookup means a future translatable-component packet only needs to send the key and args.

/// The vanilla `en_us` format string for a key.
fn template(key: &str) -> Option<&'static str> {
    Some(match key {
        "argument.entity.notfound.entity" => "No entity was found",
        "argument.entity.notfound.player" => "No player was found",
        "argument.player.unknown" => "That player does not exist",
        "argument.anchor.invalid" => "Invalid entity anchor position %s",
        "commands.ban.failed" => "Nothing changed. The player is already banned",
        "commands.ban.success" => "Banned %s: %s",
        "commands.banip.failed" => "Nothing changed. That IP is already banned",
        "commands.banip.invalid" => "Invalid IP address or unknown player",
        "commands.banip.success" => "Banned IP %s: %s",
        "commands.banlist.entry" => "%s was banned by %s: %s",
        "commands.banlist.entry.unknown" => "(Unknown)",
        "commands.banlist.list" => "There are %s ban(s):",
        "commands.banlist.none" => "There are no bans",
        "commands.deop.failed" => "Nothing changed. The player is not an operator",
        "commands.deop.success" => "Made %s no longer a server operator",
        "commands.kick.success" => "Kicked %s: %s",
        "commands.kill.success.multiple" => "Killed %s entities",
        "commands.kill.success.single" => "Killed %s",
        "commands.message.display.incoming" => "%s whispers to you: %s",
        "commands.message.display.outgoing" => "You whisper to %s: %s",
        "commands.op.failed" => "Nothing changed. The player is already an operator",
        "commands.op.success" => "Made %s a server operator",
        "commands.pardon.failed" => "Nothing changed. The player isn't banned",
        "commands.pardon.success" => "Unbanned %s",
        "commands.pardonip.failed" => "Nothing changed. That IP isn't banned",
        "commands.pardonip.invalid" => "Invalid IP address",
        "commands.pardonip.success" => "Unbanned IP %s",
        "commands.save.alreadyOff" => "Saving is already turned off",
        "commands.save.alreadyOn" => "Saving is already turned on",
        "commands.save.disabled" => "Automatic saving is now disabled",
        "commands.save.enabled" => "Automatic saving is now enabled",
        "commands.save.failed" => "Unable to save the game (is there enough disk space?)",
        "commands.save.saving" => "Saving the game (this may take a moment!)",
        "commands.save.success" => "Saved the game",
        "commands.stop.stopping" => "Stopping the server",
        "commands.teleport.invalidPosition" => "Invalid position for teleport",
        "commands.teleport.success.entity.multiple" => "Teleported %s entities to %s",
        "commands.teleport.success.entity.single" => "Teleported %s to %s",
        "commands.teleport.success.location.multiple" => "Teleported %s entities to %s, %s, %s",
        "commands.teleport.success.location.single" => "Teleported %s to %s, %s, %s",
        "commands.whitelist.add.failed" => "Player is already whitelisted",
        "commands.whitelist.add.success" => "Added %s to the whitelist",
        "commands.whitelist.alreadyOff" => "Whitelist is already turned off",
        "commands.whitelist.alreadyOn" => "Whitelist is already turned on",
        "commands.whitelist.disabled" => "Whitelist is now turned off",
        "commands.whitelist.enabled" => "Whitelist is now turned on",
        "commands.whitelist.list" => "There are %s whitelisted player(s): %s",
        "commands.whitelist.none" => "There are no whitelisted players",
        "commands.whitelist.reloaded" => "Reloaded the whitelist",
        "commands.whitelist.remove.failed" => "Player is not whitelisted",
        "commands.whitelist.remove.success" => "Removed %s from the whitelist",
        _ => return None,
    })
}

/// Renders `key` with `args`, substituting `%s` placeholders in order.
///
/// Unknown keys render as the key itself followed by the arguments; every key this server
/// emits is in the table above.
pub fn render(key: &str, args: &[&str]) -> String {
    let Some(template) = template(key) else {
        if args.is_empty() {
            return key.to_owned();
        }
        return format!("{key}: {}", args.join(", "));
    };
    let mut text = template.to_owned();
    for arg in args {
        match text.find("%s") {
            Some(index) => text.replace_range(index..index + 2, arg),
            None => break,
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_vanilla_feedback() {
        assert_eq!(
            render("commands.op.success", &["Alice"]),
            "Made Alice a server operator"
        );
        assert_eq!(
            render(
                "commands.teleport.success.location.single",
                &["Alice", "10.5", "70.0", "20.5"]
            ),
            "Teleported Alice to 10.5, 70.0, 20.5"
        );
        assert_eq!(
            render("commands.message.display.incoming", &["Server", "hi"]),
            "Server whispers to you: hi"
        );
        assert_eq!(
            render("commands.banlist.entry", &["Bob", "Server", "griefing"]),
            "Bob was banned by Server: griefing"
        );
        assert_eq!(render("commands.banlist.none", &[]), "There are no bans");
        assert_eq!(
            render("argument.entity.notfound.player", &[]),
            "No player was found"
        );
        assert_eq!(
            render("argument.anchor.invalid", &["head"]),
            "Invalid entity anchor position head"
        );
        assert_eq!(render("unknown.key", &[]), "unknown.key");
    }
}
