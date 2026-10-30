import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.PrintStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HexFormat;
import net.minecraft.SharedConstants;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.DataComponentMap;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.ProtocolInfo;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.configuration.ConfigurationProtocols;
import net.minecraft.network.protocol.game.GameProtocols;
import net.minecraft.server.Bootstrap;

/** Decodes recorded clientbound packets with the real vanilla codecs. Input lines: "<phase> <name> <hex frame>". */
public class PacketCheck {
    public static void main(String[] args) throws Exception {
        // Bootstrap redirects System.out into the game logger; keep a handle on the real stdout.
        PrintStream out = new PrintStream(new FileOutputStream(FileDescriptor.out), true);
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        // Built-in registries (entity types, items, ...) so registry-backed fields decode; data-driven
        // registries synced during configuration are still absent.
        RegistryAccess registries = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        // Item default components are bound by datapack loading, which never runs here. Stacks only
        // need a bound map to decode, so give unbound items an empty one.
        BuiltInRegistries.ITEM.listElements().filter(item -> !item.areComponentsBound())
            .forEach(item -> item.bindComponents(DataComponentMap.EMPTY));
        ProtocolInfo<?> play = GameProtocols.CLIENTBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(registries));
        ProtocolInfo<?> config = ConfigurationProtocols.CLIENTBOUND;
        int ok = 0, bad = 0;
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            String[] p = line.split(" ");
            if (p.length < 3 || p[2].equals("-")) {
                out.println("SKIP " + line);
                continue;
            }
            ByteBuf buf = Unpooled.wrappedBuffer(HexFormat.of().parseHex(p[2]));
            ProtocolInfo<?> info = p[0].equals("play") ? play : config;
            try {
                Packet<?> packet = (Packet<?>) info.codec().decode(buf);
                if (buf.readableBytes() != 0) {
                    bad++;
                    out.println("BAD  " + p[0] + " " + p[1] + " " + buf.readableBytes() + " bytes left over after decoding");
                } else {
                    ok++;
                    out.println("OK   " + p[0] + " " + p[1]);
                }
            } catch (Throwable t) {
                bad++;
                StringBuilder why = new StringBuilder();
                for (Throwable c = t; c != null; c = c.getCause()) why.append(" <- ").append(c.getClass().getSimpleName()).append(": ").append(c.getMessage());
                out.println("BAD  " + p[0] + " " + p[1] + why);
            }
        }
        out.println("SUMMARY ok=" + ok + " bad=" + bad);
        System.exit(bad == 0 ? 0 : 1);
    }
}
