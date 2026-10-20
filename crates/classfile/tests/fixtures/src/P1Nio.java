package demo;

import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.DirectoryStream;
import java.nio.file.FileSystems;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.StandardOpenOption;

// Provenance for P1Nio.class — FileSystems/FileChannel/Files/DirectoryStream APIs
// added in P1-b. Compiled with `-parameters -g --release 25` (major 69).
public class P1Nio {
    public static void main(String[] args) throws Exception {
        Path file = Files.createTempFile("p1nio", ".txt");
        Files.writeString(file, "hello");
        FileChannel channel = FileChannel.open(file, StandardOpenOption.READ);
        ByteBuffer buffer = ByteBuffer.allocate(5);
        int read = channel.read(buffer);
        buffer.flip();
        StringBuilder text = new StringBuilder();
        while (buffer.hasRemaining()) {
            text.append((char) buffer.get());
        }
        channel.close();
        int entries = 0;
        try (DirectoryStream<Path> stream = Files.newDirectoryStream(file.getParent(), "p1nio*")) {
            for (Path ignored : stream) {
                entries++;
            }
        }
        String result = FileSystems.getDefault().getSeparator()
                + ":" + file.getFileSystem().isOpen()
                + ":" + read + ":" + text
                + ":" + Files.size(file)
                + ":" + Files.probeContentType(file)
                + ":" + Files.isWritable(file)
                + ":" + Paths.get(file.toUri()).getFileName()
                + ":" + (entries > 0);
        Files.delete(file);
        System.out.println(result);
    }
}
