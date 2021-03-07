use tokio::task::{ LocalSet, yield_now };
use ritsu::{ actions, Proactor };

fn main() -> anyhow::Result<()> {
    let mut proactor = Proactor::new()?;
    let handle = proactor.handle();

    let taskset = LocalSet::new();

    ritsu::block_on(&mut proactor, async move {
        for _ in 0..500 {
            for _ in 0..500 {
                taskset.spawn_local(actions::nop(handle.clone()));
            }

            yield_now().await;
        }

        taskset.await;
    })?;

    Ok(())
}
