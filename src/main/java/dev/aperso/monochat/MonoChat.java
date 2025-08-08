package dev.aperso.monochat;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Main class for connecting to and receiving messages from chat streaming
 * platforms.
 * Supports Chzzk and Soop platforms through native Rust implementation.
 */
public class MonoChat {
    private static boolean initialized = false;

    static {
        try {
            // Load the native library from temp directory
            loadNativeLibrary();

            // Initialize the native runtime
            if (init() != 0) {
                throw new RuntimeException("Failed to initialize MonoChat native library");
            }
            initialized = true;
        } catch (Exception e) {
            throw new RuntimeException("Failed to load MonoChat native library", e);
        }
    }

    private static void loadNativeLibrary() throws IOException {
        String osName = System.getProperty("os.name").toLowerCase();
        String osArch = System.getProperty("os.arch").toLowerCase();

        // Determine the platform-specific library path
        String platformDir;
        String libraryName;

        if (osName.contains("windows")) {
            if (osArch.contains("aarch64") || osArch.contains("arm64")) {
                platformDir = "native/windows-arm64";
                libraryName = "monochat.dll";
            } else {
                platformDir = "native/windows-x64";
                libraryName = "monochat.dll";
            }
        } else if (osName.contains("linux")) {
            if (osArch.contains("aarch64") || osArch.contains("arm64")) {
                platformDir = "native/linux-arm64";
                libraryName = "libmonochat.so";
            } else {
                platformDir = "native/linux-x64";
                libraryName = "libmonochat.so";
            }
        } else if (osName.contains("mac")) {
            // Keep existing macOS support for now
            platformDir = "";
            libraryName = "libmonochat.dylib";
        } else {
            throw new UnsupportedOperationException("Unsupported platform: " + osName + " " + osArch);
        }

        // Try to load from JAR resources
        String resourcePath = "/" + (platformDir.isEmpty() ? "" : platformDir + "/") + libraryName;
        InputStream libraryStream = MonoChat.class.getResourceAsStream(resourcePath);

        if (libraryStream == null) {
            // Fallback: try to load from system library path
            System.loadLibrary("monochat");
            return;
        }

        try {
            // Create a temporary directory
            Path tempDir = Files.createTempDirectory("monochat");
            tempDir.toFile().deleteOnExit();

            // Create the temporary library file
            File tempLibrary = new File(tempDir.toFile(), libraryName);
            tempLibrary.deleteOnExit();

            // Copy the library from JAR to temporary file
            try (FileOutputStream out = new FileOutputStream(tempLibrary)) {
                byte[] buffer = new byte[8192];
                int bytesRead;
                while ((bytesRead = libraryStream.read(buffer)) != -1) {
                    out.write(buffer, 0, bytesRead);
                }
            }

            // Load the library from the temporary file
            System.load(tempLibrary.getAbsolutePath());

        } finally {
            libraryStream.close();
        }
    }

    /**
     * Connects to a Chzzk chat stream.
     * 
     * @param url The Chzzk API URL (either chat or live status endpoint)
     *            Examples:
     *            - Chat: https://api.chzzk.naver.com/manage/v1/chats/sources/{uuid}
     *            - Live:
     *            https://api.chzzk.naver.com/polling/v3.1/channels/{uuid}/live-status
     * @return A MessageStream for receiving messages, or null if connection failed
     */
    public static MessageStream connectChzzk(String url) {
        ensureInitialized();
        if (url == null) {
            throw new IllegalArgumentException("URL cannot be null");
        }

        long streamId = connectChzzkNative(url);
        if (streamId < 0) {
            return null;
        }

        return new MessageStream(streamId);
    }

    /**
     * Connects to a Soop chat stream.
     * 
     * @param aquaUrl The Soop Aqua API URL with query parameters
     *                Example:
     *                https://aqua.sooplive.co.kr/component.php?szKey={key}
     * @return A MessageStream for receiving messages, or null if connection failed
     */
    public static MessageStream connectSoop(String aquaUrl) {
        ensureInitialized();
        if (aquaUrl == null) {
            throw new IllegalArgumentException("URL cannot be null");
        }

        long streamId = connectSoopNative(aquaUrl);
        if (streamId < 0) {
            return null;
        }

        return new MessageStream(streamId);
    }

    private static void ensureInitialized() {
        if (!initialized) {
            throw new IllegalStateException("MonoChat not properly initialized");
        }
    }

    static Message getNextMessage(long streamId) {
        long messagePtr = nextMessage(streamId);
        if (messagePtr == 0) {
            return null; // No message available
        }
        if (messagePtr < 0) {
            throw new IllegalStateException("Invalid stream ID");
        }
        return new Message(messagePtr);
    }

    static void closeStreamInternal(long streamId) {
        closeStreamNative(streamId);
    }

    // Native methods
    private static native int init();

    private static native long connectChzzkNative(String url);

    private static native long connectSoopNative(String url);

    private static native long nextMessage(long streamId);

    private static native int closeStreamNative(long streamId);
}
