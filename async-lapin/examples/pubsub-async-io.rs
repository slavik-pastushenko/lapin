use async_lapin::*;
use lapin::{
    executor::Executor, message::DeliveryResult, options::*, publisher_confirm::Confirmation,
    types::FieldTable, BasicProperties, Connection, ConnectionProperties, Result,
};
use log::info;
use std::{future::Future, pin::Pin};

#[derive(Debug)]
struct SmolExecutor;

impl Executor for SmolExecutor {
    fn spawn(&self, f: Pin<Box<dyn Future<Output = ()> + Send>>) {
        smol::spawn(f).detach();
    }

    fn spawn_blocking(&self, f: Box<dyn FnOnce() + Send>) -> Result<()> {
        smol::spawn(blocking::unblock(f)).detach();
        Ok(())
    }
}

fn main() -> Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    env_logger::init();

    let addr = std::env::var("AMQP_ADDR").unwrap_or_else(|_| "amqp://127.0.0.1:5672/%2f".into());

    smol::block_on(async {
        let conn = Connection::connect(
            &addr,
            ConnectionProperties::default().with_async_io(SmolExecutor),
        )
        .await?;

        info!("CONNECTED");

        let channel_a = conn.create_channel().await?;
        let channel_b = conn.create_channel().await?;

        let queue = channel_a
            .queue_declare(
                "hello",
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

        info!("Declared queue {:?}", queue);

        let consumer = channel_b
            .basic_consume(
                "hello",
                "my_consumer",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;

        consumer.set_delegate(move |delivery: DeliveryResult| async move {
            let delivery = delivery.expect("error caught in in consumer");
            if let Some((channel, delivery)) = delivery {
                channel
                    .basic_ack(delivery.delivery_tag, BasicAckOptions::default())
                    .await
                    .expect("failed to ack");
            }
        });

        let payload = b"Hello world!";

        loop {
            let confirm = channel_a
                .basic_publish(
                    "",
                    "hello",
                    BasicPublishOptions::default(),
                    payload.to_vec(),
                    BasicProperties::default(),
                )
                .await?
                .await?;
            assert_eq!(confirm, Confirmation::NotRequested);
        }
    })
}