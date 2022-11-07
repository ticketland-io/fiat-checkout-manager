use std::{
  sync::Arc,
  env,
  panic,
  process,
};
use actix::prelude::*;
use amqp_helpers::consumer::consumer_runner::ConsumerRunner;
use fiat_checkout_manager::{
  utils::store::Store,
  queue::create_checkout_consumer::CreateCheckoutHandler,
};


fn main() {
  let orig_hook = panic::take_hook();
  panic::set_hook(Box::new(move |panic_info| {
    orig_hook(panic_info);
    process::exit(1);
  }));

  if env::var("ENV").unwrap() == "development" {
    dotenv::from_filename(".env").expect("cannot load env from a file");
  }

  tracing_subscriber::fmt::init();
  
  let system = System::new();

  let execution = async {
    let store = Arc::new(Store::new().await);

    let mut role_handler_consumer = ConsumerRunner::new(
      store.config.rabbitmq_uri.clone(),
      "checkout_session".to_owned(),
      "checkout_session".to_owned(),
      Arc::new(CreateCheckoutHandler::new(store).await),
    ).await;

    role_handler_consumer.start().await.unwrap();
  };

  let arbiter = Arbiter::new();
  arbiter.spawn(execution);
  system.run().expect("Could not run the actix-rt system");
}
