// SPDX-License-Identifier: MIT OR Apache-2.0

import bolts.CancellationTokenSource;
import bolts.Task;
import bolts.TaskCompletionSource;
import java.util.concurrent.atomic.AtomicInteger;

/** JVM diagnostic of the source replacement's task/cancellation seam, not Android runtime evidence. */
public final class BoltsSourceContract {
  public static void main(String[] args) throws Exception {
    TaskCompletionSource<Integer> source = new TaskCompletionSource<>();
    Task<Integer> chained = source.getTask().continueWith(task -> task.getResult() + 1, Runnable::run);
    source.setResult(41);
    if (!chained.isCompleted() || chained.getResult() != 42) throw new AssertionError("continuation");

    Task<Integer> flat = Task.forResult(4).onSuccessTask(task -> Task.forResult(task.getResult() * 2), Runnable::run);
    if (flat.getResult() != 8) throw new AssertionError("Fresco-style task continuation");
    Task<Integer> failed = Task.forError(new IllegalStateException("expected"));
    if (!failed.isFaulted() || !(failed.getError() instanceof IllegalStateException)) throw new AssertionError("error");

    CancellationTokenSource cancelled = new CancellationTokenSource();
    AtomicInteger invoked = new AtomicInteger();
    cancelled.getToken().register(invoked::incrementAndGet);
    cancelled.cancel();
    if (!cancelled.isCancellationRequested() || invoked.get() != 1) throw new AssertionError("cancellation");
    cancelled.close();

    CancellationTokenSource closed = new CancellationTokenSource();
    closed.getToken().register(invoked::incrementAndGet);
    closed.getToken().register(invoked::incrementAndGet);
    closed.getToken().register(invoked::incrementAndGet);
    // The post-1.4.0 source snapshots registrations before close mutates the collection.
    closed.close();
    if (invoked.get() != 1) throw new AssertionError("close must not invoke callbacks");
    System.out.println("Bolts source task/cancellation contract passed");
  }
}
