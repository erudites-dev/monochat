package dev.aperso.monochat;

/**
 * Example demonstrating how to use the MonoChat library.
 */
public class Example {
    public static void main(String[] args) {
        if (args.length < 2) {
            System.out.println("Usage: java -cp <classpath> dev.aperso.monochat.Example <platform> <url>");
            System.out.println("Platform: chzzk or soop");
            return;
        }

        String platform = args[0].toLowerCase();
        String url = args[1];

        MessageStream stream = null;
        try {
            // Connect to the appropriate platform
            switch (platform) {
                case "chzzk":
                    System.out.println("Connecting to Chzzk: " + url);
                    stream = MonoChat.connectChzzk(url);
                    break;
                case "soop":
                    System.out.println("Connecting to Soop: " + url);
                    stream = MonoChat.connectSoop(url);
                    break;
                default:
                    System.err.println("Unknown platform: " + platform);
                    System.err.println("Supported platforms: chzzk, soop");
                    return;
            }

            if (stream == null) {
                System.err.println("Failed to connect to " + platform + " stream");
                return;
            }

            System.out.println("Connected! Listening for messages...");
            System.out.println("Press Ctrl+C to stop");

            while (!stream.isClosed()) {
                try {
                    Message message = stream.nextMessage(1000, java.util.concurrent.TimeUnit.MILLISECONDS);
                    if (message != null) {
                        System.out.format("%s : %s (%d)\n",
                                message.getSender(),
                                message.getContent(),
                                message.getDonated());
                        // Free the message when done with it
                        message.free();
                    }
                } catch (InterruptedException e) {
                    System.out.println("\nInterrupted, stopping...");
                    break;
                }
            }

        } catch (Exception e) {
            System.err.println("Error: " + e.getMessage());
            e.printStackTrace();
        } finally {
            if (stream != null) {
                stream.close();
                System.out.println("Stream closed");
            }
        }
    }
}
