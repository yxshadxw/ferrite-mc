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
        "commands.effect.clear.everything.failed" => "Target has no effects to remove",
        "commands.effect.clear.everything.success.multiple" => {
            "Removed every effect from %s targets"
        }
        "commands.effect.clear.everything.success.single" => "Removed every effect from %s",
        "commands.effect.clear.specific.failed" => "Target doesn't have the requested effect",
        "commands.effect.clear.specific.success.multiple" => "Removed effect %s from %s targets",
        "commands.effect.clear.specific.success.single" => "Removed effect %s from %s",
        "commands.effect.give.failed" => {
            "Unable to apply this effect (target is either immune to effects, or has something stronger)"
        }
        "commands.effect.give.success.multiple" => "Applied effect %s to %s targets",
        "commands.effect.give.success.single" => "Applied effect %s to %s",
        "commands.enchant.failed" => {
            "Nothing changed. Targets either have no item in their hands or the enchantment could not be applied"
        }
        "commands.enchant.failed.entity" => "%s is not a valid entity for this command",
        "commands.enchant.failed.incompatible" => "%s cannot support that enchantment",
        "commands.enchant.failed.itemless" => "%s is not holding any item",
        "commands.enchant.failed.level" => {
            "%s is higher than the maximum level of %s supported by that enchantment"
        }
        "commands.enchant.success.multiple" => "Applied enchantment %s to %s entities",
        "commands.enchant.success.single" => "Applied enchantment %s to %s's item",
        "commands.title.cleared.multiple" => "Cleared titles for %s players",
        "commands.title.cleared.single" => "Cleared titles for %s",
        "commands.title.reset.multiple" => "Reset title options for %s players",
        "commands.title.reset.single" => "Reset title options for %s",
        "commands.title.show.actionbar.multiple" => "Showing new actionbar title for %s players",
        "commands.title.show.actionbar.single" => "Showing new actionbar title for %s",
        "commands.title.show.subtitle.multiple" => "Showing new subtitle for %s players",
        "commands.title.show.subtitle.single" => "Showing new subtitle for %s",
        "commands.title.show.title.multiple" => "Showing new title for %s players",
        "commands.title.show.title.single" => "Showing new title for %s",
        "commands.title.times.multiple" => "Changed title display times for %s players",
        "commands.title.times.single" => "Changed title display times for %s",
        "commands.playsound.failed" => "The sound is too far away to be heard",
        "commands.playsound.success.multiple" => "Played sound %s to %s players",
        "commands.playsound.success.single" => "Played sound %s to %s",
        "commands.stopsound.success.source.any" => "Stopped all '%s' sounds",
        "commands.stopsound.success.source.sound" => "Stopped sound '%s' on source '%s'",
        "commands.stopsound.success.sourceless.any" => "Stopped all sounds",
        "commands.stopsound.success.sourceless.sound" => "Stopped sound '%s'",
        "commands.particle.failed" => "The particle was not visible for anybody",
        "commands.particle.success" => "Displaying particle %s",
        "commands.spawnpoint.success.multiple" => {
            "Set spawn point to %s, %s, %s [%s] in %s for %s players"
        }
        "commands.spawnpoint.success.multiple.new" => {
            "Set spawn point to %s, %s, %s [%s, %s] in %s for %s players"
        }
        "commands.spawnpoint.success.single" => "Set spawn point to %s, %s, %s [%s] in %s for %s",
        "commands.spawnpoint.success.single.new" => {
            "Set spawn point to %s, %s, %s [%s, %s] in %s for %s"
        }
        "commands.setworldspawn.failure.not_overworld" => {
            "Can only set the world spawn for the Overworld"
        }
        "commands.setworldspawn.success" => "Set the world spawn point to %s, %s, %s [%s]",
        "commands.setworldspawn.success.new" => {
            "Set the world spawn point to %s, %s, %s [%s, %s] in %s"
        }
        "commands.worldborder.center.failed" => {
            "Nothing changed. The world border is already centered there"
        }
        "commands.worldborder.center.success" => "Set the center of the world border to %s, %s",
        "commands.worldborder.damage.amount.failed" => {
            "Nothing changed. The world border damage is already that amount"
        }
        "commands.worldborder.damage.amount.success" => {
            "Set the world border damage to %s per block each second"
        }
        "commands.worldborder.damage.buffer.failed" => {
            "Nothing changed. The world border damage buffer is already that distance"
        }
        "commands.worldborder.damage.buffer.success" => {
            "Set the world border damage buffer to %s block(s)"
        }
        "commands.worldborder.get" => "The world border is currently %s block(s) wide",
        "commands.worldborder.set.failed.big" => {
            "World border cannot be bigger than %s blocks wide"
        }
        "commands.worldborder.set.failed.far" => {
            "World border cannot be further out than %s blocks"
        }
        "commands.worldborder.set.failed.nochange" => {
            "Nothing changed. The world border is already that size"
        }
        "commands.worldborder.set.failed.small" => {
            "World border cannot be smaller than 1 block wide"
        }
        "commands.worldborder.set.grow" => {
            "Growing the world border to %s block(s) wide over %s second(s)"
        }
        "commands.worldborder.set.immediate" => "Set the world border to %s block(s) wide",
        "commands.worldborder.set.shrink" => {
            "Shrinking the world border to %s block(s) wide over %s second(s)"
        }
        "commands.worldborder.warning.distance.failed" => {
            "Nothing changed. The world border warning is already that distance"
        }
        "commands.worldborder.warning.distance.success" => {
            "Set the world border warning distance to %s block(s)"
        }
        "commands.worldborder.warning.time.failed" => {
            "Nothing changed. The world border warning is already that amount of time"
        }
        "commands.worldborder.warning.time.success" => {
            "Set the world border warning time to %s second(s)"
        }
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
    let mut parts = template.split("%s");
    let mut text = parts.next().unwrap_or_default().to_owned();
    for (arg, part) in args.iter().zip(parts) {
        text.push_str(arg);
        text.push_str(part);
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
