// Encodes representative 26.3 item stacks with the vanilla server's network Slot codec and
// prints `name=hex` lines. The Rust integration test compiles this file against the local server
// jar and re-encodes each line with ferrite-item; no Mojang code or output is committed.
//
// Item components are data-driven in 26.3, so a bare JVM has no bound defaults. Binding an empty
// component map to the item holders is enough for the codec: every stack below sets exactly the
// components it wants to exercise.

import com.mojang.authlib.GameProfile;
import io.netty.buffer.Unpooled;
import it.unimi.dsi.fastutil.ints.IntList;
import java.util.List;
import java.util.Optional;
import java.util.UUID;
import net.minecraft.SharedConstants;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderSet;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.DataComponentMap;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.network.Filterable;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.EquipmentSlotGroup;
import net.minecraft.world.entity.ai.attributes.AttributeModifier;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.decoration.painting.PaintingVariant;
import net.minecraft.world.food.FoodProperties;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.ItemStackTemplate;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.JukeboxPlayable;
import net.minecraft.world.item.ItemUseAnimation;
import net.minecraft.world.item.JukeboxSong;
import net.minecraft.world.item.Instrument;
import net.minecraft.world.item.component.BlocksAttacks;
import net.minecraft.world.item.component.Consumable;
import net.minecraft.world.item.consume_effects.ClearAllStatusEffectsConsumeEffect;
import net.minecraft.world.item.alchemy.PotionContents;
import net.minecraft.world.item.component.BundleContents;
import net.minecraft.world.item.component.CustomModelData;
import net.minecraft.world.item.component.FireworkExplosion;
import net.minecraft.world.item.component.InstrumentComponent;
import net.minecraft.world.item.component.ItemAttributeModifiers;
import net.minecraft.world.item.component.ItemContainerContents;
import net.minecraft.world.item.component.ItemLore;
import net.minecraft.world.item.component.ResolvableProfile;
import net.minecraft.world.item.component.Tool;
import net.minecraft.world.item.component.UseEffects;
import net.minecraft.world.item.component.WrittenBookContent;
import net.minecraft.world.item.equipment.Equippable;
import net.minecraft.world.item.equipment.trim.ArmorTrim;
import net.minecraft.world.item.equipment.trim.TrimMaterial;
import net.minecraft.world.item.equipment.trim.TrimPattern;
import net.minecraft.world.level.block.Blocks;

public final class SlotDump {
    private static RegistryAccess.Frozen access;

    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        access = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        BuiltInRegistries.ITEM.stream()
            .forEach(item -> item.builtInRegistryHolder().bindComponents(DataComponentMap.EMPTY));

        ItemStack sword = new ItemStack(Items.DIAMOND_SWORD);
        sword.set(DataComponents.CUSTOM_NAME, Component.literal("Excalibur"));
        sword.set(DataComponents.LORE, new ItemLore(List.of(Component.literal("a"), Component.literal("b"))));
        sword.set(DataComponents.DAMAGE, 7);
        emit("sword", sword);

        ItemStack stone = new ItemStack(Items.STONE, 3);
        stone.set(DataComponents.MAX_STACK_SIZE, 16);
        stone.set(DataComponents.MAX_DAMAGE, 1561);
        stone.set(DataComponents.UNBREAKABLE, net.minecraft.util.Unit.INSTANCE);
        stone.set(DataComponents.REPAIR_COST, 3);
        emit("stone", stone);

        ItemStack shield = new ItemStack(Items.SHIELD);
        shield.set(DataComponents.BLOCKS_ATTACKS, new BlocksAttacks(
            0.25F, 1.0F,
            List.of(new BlocksAttacks.DamageReduction(90.0F, Optional.empty(), 0.0F, 1.0F)),
            new BlocksAttacks.ItemDamageFunction(1.0F, 1.0F, 1.0F),
            Optional.empty(), Optional.of(SoundEvents.ARMOR_EQUIP_IRON), Optional.empty()));
        emit("blocks_attacks", shield);

        ItemStack consumable = new ItemStack(Items.APPLE);
        consumable.set(DataComponents.CONSUMABLE, new Consumable(
            1.6F, ItemUseAnimation.EAT, SoundEvents.ARMOR_EQUIP_IRON, false,
            List.of(ClearAllStatusEffectsConsumeEffect.INSTANCE)));
        emit("consumable", consumable);

        ItemStack food = new ItemStack(Items.APPLE);
        food.set(DataComponents.FOOD, new FoodProperties(4, 2.4F, true));
        food.set(DataComponents.USE_EFFECTS, new UseEffects(true, false, 0.5F));
        emit("food", food);

        ItemStack model = new ItemStack(Items.PAPER);
        model.set(DataComponents.CUSTOM_MODEL_DATA,
            new CustomModelData(List.of(1.5F), List.of(true), List.of("a"), List.of(7)));
        emit("model", model);

        ItemStack tool = new ItemStack(Items.IRON_PICKAXE);
        tool.set(DataComponents.TOOL, new Tool(
            List.of(new Tool.Rule(
                HolderSet.direct(Blocks.STONE.builtInRegistryHolder()),
                Optional.of(8.0F),
                Optional.of(true))),
            1.0F, 1, true));
        emit("tool", tool);

        ItemStack attributes = new ItemStack(Items.IRON_SWORD);
        attributes.set(DataComponents.ATTRIBUTE_MODIFIERS, new ItemAttributeModifiers(List.of(
            new ItemAttributeModifiers.Entry(
                Attributes.ATTACK_DAMAGE,
                new AttributeModifier(
                    Identifier.parse("minecraft:x"),
                    1.5,
                    AttributeModifier.Operation.ADD_VALUE),
                EquipmentSlotGroup.MAINHAND))));
        emit("attributes", attributes);

        ItemStack equippable = new ItemStack(Items.IRON_CHESTPLATE);
        equippable.set(DataComponents.EQUIPPABLE, new Equippable(
            EquipmentSlot.CHEST,
            SoundEvents.ARMOR_EQUIP_IRON,
            Optional.empty(),
            Optional.empty(),
            Optional.empty(),
            true, false, true, false, true,
            SoundEvents.ITEM_BREAK));
        emit("equippable", equippable);

        ItemStack trim = new ItemStack(Items.DIAMOND_CHESTPLATE);
        trim.set(DataComponents.TRIM, new ArmorTrim(
            new Holder.Direct<>(new TrimMaterial(Identifier.parse("minecraft:trim_material"),
                Component.literal("m")), DataComponentMap.EMPTY),
            new Holder.Direct<>(new TrimPattern(Identifier.parse("minecraft:trim_pattern"),
                Component.literal("p"), true), DataComponentMap.EMPTY)));
        emit("trim", trim);

        ItemStack profile = new ItemStack(Items.PLAYER_HEAD);
        profile.set(DataComponents.PROFILE, ResolvableProfile.createResolved(
            new GameProfile(UUID.fromString("01234567-89ab-cdef-0123-456789abcdef"), "Steve")));
        emit("profile", profile);

        ItemStack container = new ItemStack(Items.CHEST);
        container.set(DataComponents.CONTAINER, ItemContainerContents.fromItems(
            List.of(new ItemStack(Items.STONE, 2), new ItemStack(Items.DIRT))));
        emit("container", container);

        ItemStack bundle = new ItemStack(Items.BUNDLE);
        bundle.set(DataComponents.BUNDLE_CONTENTS, new BundleContents(List.of(
            new ItemStackTemplate(Items.STONE.builtInRegistryHolder(), 2, DataComponentPatch.EMPTY),
            new ItemStackTemplate(Items.DIRT.builtInRegistryHolder(), 1, DataComponentPatch.EMPTY))));
        emit("bundle", bundle);

        ItemStack book = new ItemStack(Items.WRITTEN_BOOK);
        book.set(DataComponents.WRITTEN_BOOK_CONTENT, new WrittenBookContent(
            Filterable.passThrough("T"), "A", 1,
            List.of(Filterable.passThrough(Component.literal("page"))), true));
        emit("book", book);

        ItemStack firework = new ItemStack(Items.FIREWORK_ROCKET);
        firework.set(DataComponents.FIREWORK_EXPLOSION, new FireworkExplosion(
            FireworkExplosion.Shape.STAR, IntList.of(0xFF0000), IntList.of(), true, false));
        emit("firework", firework);

        ItemStack potion = new ItemStack(Items.POTION);
        potion.set(DataComponents.POTION_CONTENTS, new PotionContents(
            Optional.empty(), Optional.of(0x11223344), List.of(), Optional.of("potion")));
        emit("potion", potion);

        ItemStack instrument = new ItemStack(Items.GOAT_HORN);
        instrument.set(DataComponents.INSTRUMENT, new InstrumentComponent(new Holder.Direct<>(
            new Instrument(SoundEvents.ARMOR_EQUIP_IRON, 1.0F, 16.0F, 1, Component.literal("i")),
            DataComponentMap.EMPTY)));
        emit("instrument", instrument);

        ItemStack jukebox = new ItemStack(Items.MUSIC_DISC_13);
        jukebox.set(DataComponents.JUKEBOX_PLAYABLE, new JukeboxPlayable(new Holder.Direct<>(
            new JukeboxSong(SoundEvents.ARMOR_EQUIP_IRON, Component.literal("song"), 10.0F, 1),
            DataComponentMap.EMPTY)));
        emit("jukebox", jukebox);

        ItemStack painting = new ItemStack(Items.PAINTING);
        painting.set(DataComponents.PAINTING_VARIANT, new Holder.Direct<>(
            new PaintingVariant(1, 2, Identifier.parse("minecraft:x"), Optional.empty(),
                Optional.empty()), DataComponentMap.EMPTY));
        emit("painting", painting);

        ItemStack skin = new ItemStack(Items.PLAYER_HEAD);
        skin.set(DataComponents.PROFILE, ResolvableProfile.createUnresolved("Alex"));
        emit("profile_unresolved", skin);
    }

    private static void emit(String name, ItemStack stack) {
        var buf = Unpooled.buffer();
        var registryBuf = new RegistryFriendlyByteBuf(buf, access);
        ItemStack.STREAM_CODEC.encode(registryBuf, stack);
        byte[] bytes = new byte[buf.readableBytes()];
        buf.readBytes(bytes);
        System.out.println(name + "=" + java.util.HexFormat.of().formatHex(bytes));
    }

    private SlotDump() {}
}
