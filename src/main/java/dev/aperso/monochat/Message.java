package dev.aperso.monochat;

import java.lang.ref.Cleaner;

/**
 * Represents a chat message from a streaming platform.
 * This class is backed by native Rust code via JNI.
 */
public class Message {
    private static final Cleaner cleaner = Cleaner.create();

    private long nativePtr;
    private final Cleaner.Cleanable cleanable;

    // Package-private constructor used by MonoChat
    Message(long nativePtr) {
        this.nativePtr = nativePtr;
        this.cleanable = cleaner.register(this, new MessageCleanupAction(nativePtr));
    }

    // Cleanup action that runs when the Message is garbage collected
    private static class MessageCleanupAction implements Runnable {
        private final long ptr;

        MessageCleanupAction(long ptr) {
            this.ptr = ptr;
        }

        @Override
        public void run() {
            if (ptr != 0) {
                free(ptr);
            }
        }
    }

    /**
     * Gets the sender/username of the message.
     * 
     * @return The sender's name/username
     */
    public String getSender() {
        if (nativePtr == 0) {
            throw new IllegalStateException("Message has been freed");
        }
        return getSender(nativePtr);
    }

    /**
     * Gets the content/text of the message.
     * 
     * @return The message content, or null if this is a donation-only message
     */
    public String getContent() {
        if (nativePtr == 0) {
            throw new IllegalStateException("Message has been freed");
        }
        return getContent(nativePtr);
    }

    /**
     * Gets the donation amount if this message includes a donation.
     * 
     * @return The donation amount, or 0 if no donation
     */
    public long getDonated() {
        if (nativePtr == 0) {
            throw new IllegalStateException("Message has been freed");
        }
        return getDonated(nativePtr);
    }

    /**
     * Checks if this message includes a donation.
     * 
     * @return true if the message has a donation, false otherwise
     */
    public boolean hasDonation() {
        if (nativePtr == 0) {
            throw new IllegalStateException("Message has been freed");
        }
        return hasDonation(nativePtr) != 0;
    }

    /**
     * Checks if this message has text content.
     * 
     * @return true if the message has content, false otherwise
     */
    public boolean hasContent() {
        return getContent() != null;
    }

    /**
     * Frees the native memory associated with this message.
     * After calling this method, the message object becomes unusable.
     */
    public void free() {
        if (nativePtr != 0) {
            cleanable.clean(); // This will call the cleanup action
            nativePtr = 0;
        }
    }

    @Override
    public String toString() {
        if (nativePtr == 0) {
            return "Message[freed]";
        }

        StringBuilder sb = new StringBuilder("Message[");
        sb.append("sender=").append(getSender());

        String content = getContent();
        if (content != null) {
            sb.append(", content='").append(content).append("'");
        }

        if (hasDonation()) {
            sb.append(", donated=").append(getDonated());
        }

        sb.append("]");
        return sb.toString();
    }

    // Native methods
    private static native String getSender(long ptr);

    private static native String getContent(long ptr);

    private static native long getDonated(long ptr);

    private static native int hasDonation(long ptr);

    private static native void free(long ptr);
}
