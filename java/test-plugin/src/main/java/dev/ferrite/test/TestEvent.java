package dev.ferrite.test;
import org.bukkit.event.*;
public final class TestEvent extends Event {
    private static final HandlerList HANDLERS = new HandlerList();
    public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
