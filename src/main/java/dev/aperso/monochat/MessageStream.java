package dev.aperso.monochat;

import java.lang.ref.Cleaner;
import java.util.Iterator;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;

/**
 * Represents an active message stream connection to a streaming platform.
 * Provides methods to receive messages from the stream.
 */
public class MessageStream implements AutoCloseable, Iterable<Message> {
    private static final Cleaner cleaner = Cleaner.create();

    private final long streamId;
    private volatile boolean closed = false;
    private final BlockingQueue<Message> messageBuffer = new LinkedBlockingQueue<>();
    private Thread pollingThread;
    private final Cleaner.Cleanable cleanable;

    // Package-private constructor
    MessageStream(long streamId) {
        this.streamId = streamId;
        this.cleanable = cleaner.register(this, new StreamCleanupAction(streamId));
        startPolling();
    }

    // Cleanup action that runs when the MessageStream is garbage collected
    private static class StreamCleanupAction implements Runnable {
        private final long streamId;

        StreamCleanupAction(long streamId) {
            this.streamId = streamId;
        }

        @Override
        public void run() {
            MonoChat.closeStreamInternal(streamId);
        }
    }

    /**
     * Gets the next message from the stream, blocking until one is available.
     * 
     * @return The next message, or null if the stream is closed
     * @throws InterruptedException if the thread is interrupted while waiting
     */
    public Message nextMessage() throws InterruptedException {
        if (closed) {
            return null;
        }
        return messageBuffer.take();
    }

    /**
     * Gets the next message from the stream with a timeout.
     * 
     * @param timeout The maximum time to wait
     * @param unit    The time unit of the timeout argument
     * @return The next message, or null if no message is available within the
     *         timeout or the stream is closed
     * @throws InterruptedException if the thread is interrupted while waiting
     */
    public Message nextMessage(long timeout, TimeUnit unit) throws InterruptedException {
        if (closed) {
            return null;
        }
        return messageBuffer.poll(timeout, unit);
    }

    /**
     * Tries to get the next message without blocking.
     * 
     * @return The next message if available, or null if no message is ready or the
     *         stream is closed
     */
    public Message tryNextMessage() {
        if (closed) {
            return null;
        }
        return messageBuffer.poll();
    }

    /**
     * Checks if the stream is closed.
     * 
     * @return true if the stream is closed, false otherwise
     */
    public boolean isClosed() {
        return closed;
    }

    /**
     * Gets the number of messages currently buffered.
     * 
     * @return The number of buffered messages
     */
    public int getBufferedMessageCount() {
        return messageBuffer.size();
    }

    /**
     * Closes the stream and releases associated resources.
     * After calling this method, no more messages will be received.
     */
    @Override
    public void close() {
        if (!closed) {
            closed = true;
            cleanable.clean(); // This will call the cleanup action

            if (pollingThread != null) {
                pollingThread.interrupt();
                try {
                    pollingThread.join(1000); // Wait up to 1 second
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                }
            }

            // Clear any remaining messages and free their memory
            Message msg;
            while ((msg = messageBuffer.poll()) != null) {
                msg.free();
            }
        }
    }

    /**
     * Returns an iterator that yields messages from this stream.
     * The iterator will block on next() calls until messages are available.
     * When the stream is closed, the iterator will finish.
     */
    @Override
    public Iterator<Message> iterator() {
        return new MessageIterator();
    }

    private void startPolling() {
        pollingThread = new Thread(() -> {
            try {
                while (!closed && !Thread.currentThread().isInterrupted()) {
                    Message message = MonoChat.getNextMessage(streamId);
                    if (message != null) {
                        if (!messageBuffer.offer(message)) {
                            // This should not happen with LinkedBlockingQueue, but just in case
                            message.free();
                        }
                    } else {
                        // No message available, sleep briefly to avoid busy waiting
                        try {
                            Thread.sleep(10);
                        } catch (InterruptedException e) {
                            Thread.currentThread().interrupt();
                            break;
                        }
                    }
                }
            } catch (Exception e) {
                // Log error and close stream
                System.err.println("Error in chat stream polling: " + e.getMessage());
                close();
            }
        }, "MessageStream-" + streamId);

        pollingThread.setDaemon(true);
        pollingThread.start();
    }

    private class MessageIterator implements Iterator<Message> {
        private Message nextMessage = null;
        private boolean hasCheckedNext = false;

        @Override
        public boolean hasNext() {
            if (!hasCheckedNext) {
                try {
                    nextMessage = MessageStream.this.nextMessage();
                    hasCheckedNext = true;
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                    return false;
                }
            }
            return nextMessage != null;
        }

        @Override
        public Message next() {
            if (!hasNext()) {
                return null;
            }
            Message result = nextMessage;
            nextMessage = null;
            hasCheckedNext = false;
            return result;
        }
    }
}
