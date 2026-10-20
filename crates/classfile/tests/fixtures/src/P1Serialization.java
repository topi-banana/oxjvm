package demo;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.ObjectInputStream;
import java.io.ObjectOutputStream;
import java.util.jar.Attributes;
import java.util.jar.JarEntry;
import java.util.jar.JarOutputStream;
import java.util.jar.Manifest;

// Provenance for P1Serialization.class — object streams and jar/manifest APIs in P1-b.
// Compiled with `-parameters -g --release 25` (major 69).
public class P1Serialization {
    public static String objectStream() throws Exception {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        ObjectOutputStream out = new ObjectOutputStream(bytes);
        out.writeObject("hello");
        out.writeInt(7);
        out.writeUTF("world");
        out.close();
        ObjectInputStream in = new ObjectInputStream(new ByteArrayInputStream(bytes.toByteArray()));
        String first = (String) in.readObject();
        int second = in.readInt();
        String third = in.readUTF();
        in.close();
        return first + ":" + second + ":" + third;
    }

    public static String manifest() throws Exception {
        Manifest manifest = new Manifest();
        manifest.getMainAttributes().put(Attributes.Name.MANIFEST_VERSION, "1.0");
        manifest.getMainAttributes().putValue("Main-Class", "demo.Main");
        return manifest.getMainAttributes().getValue("Manifest-Version")
                + ":" + manifest.getMainAttributes().getValue("Main-Class");
    }

    public static String jar() throws Exception {
        Manifest manifest = new Manifest();
        manifest.getMainAttributes().put(Attributes.Name.MANIFEST_VERSION, "1.0");
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        JarOutputStream jar = new JarOutputStream(bytes, manifest);
        jar.putNextEntry(new JarEntry("a.txt"));
        jar.write("data".getBytes());
        jar.closeEntry();
        jar.close();
        return String.valueOf(bytes.size() > 0);
    }
}
