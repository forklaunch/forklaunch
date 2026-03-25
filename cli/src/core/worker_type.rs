use crate::constants::WorkerType;

pub(crate) fn get_worker_type_name(r#type: &WorkerType) -> String {
    match r#type {
        WorkerType::BullMQCache => "BullMq".to_string(),
        WorkerType::Database => "Database".to_string(),
        WorkerType::Kafka => "Kafka".to_string(),
        WorkerType::RedisCache => "Redis".to_string(),
    }
}

const DEFAULT_BULLMQ_WORKER_OPTIONS: &str = "factory: ({ REDIS_URL }) => ({
  backoffType: 'exponential' as const,
  queueOptions: {
    connection: {
      url: REDIS_URL
    }
  },
  retries: 3,
  interval: 5000
})";
const DEFAULT_DATABASE_WORKER_OPTIONS: &str = "value: {
  retries: 3,
  interval: 5000
}";
const DEFAULT_KAFKA_WORKER_OPTIONS: &str =
    "factory: ({ KAFKA_BROKERS, KAFKA_CLIENT_ID, KAFKA_GROUP_ID }) => ({
  brokers: KAFKA_BROKERS,
  clientId: KAFKA_CLIENT_ID,
  groupId: KAFKA_GROUP_ID,
  retries: 3,
  interval: 5000,
  peekCount: 100
})";
const DEFAULT_REDIS_WORKER_OPTIONS: &str = "factory: ({ REDIS_URL }) => ({
  backoffType: 'exponential' as const,
  connection: {
    url: REDIS_URL
  },
  retries: 3,
  interval: 5000,
  pageSize: 100
})";

pub(crate) fn get_default_worker_options(r#type: &WorkerType) -> String {
    match r#type {
        WorkerType::BullMQCache => DEFAULT_BULLMQ_WORKER_OPTIONS.to_string(),
        WorkerType::Database => DEFAULT_DATABASE_WORKER_OPTIONS.to_string(),
        WorkerType::Kafka => DEFAULT_KAFKA_WORKER_OPTIONS.to_string(),
        WorkerType::RedisCache => DEFAULT_REDIS_WORKER_OPTIONS.to_string(),
    }
}

fn get_database_worker_consumer_factory(pascal_case_name: &str) -> String {
    format!(
        "
({{ EntityMgr, WorkerOptions }}) =>
  (
    processEventsFunction: WorkerProcessFunction<{}EventRecord>,
    failureHandler: WorkerFailureHandler<{}EventRecord>
  ) =>
    new DatabaseWorkerConsumer(
      {}EventRecord,
      EntityMgr,
      WorkerOptions,
      processEventsFunction,
      failureHandler
    )",
        pascal_case_name, pascal_case_name, pascal_case_name
    )
}
fn get_bullmq_worker_consumer_factory(_pascal_case_name: &str) -> String {
    "({ QUEUE_NAME, WorkerOptions }) =>
  (
    processEventsFunction: WorkerProcessFunction<EncryptedEventEnvelope>,
    failureHandler: WorkerFailureHandler<EncryptedEventEnvelope>
  ) =>
    new BullMqWorkerConsumer(
      QUEUE_NAME,
      WorkerOptions,
      processEventsFunction,
      failureHandler
    )".to_string()
}
fn get_kafka_worker_consumer_factory(_pascal_case_name: &str) -> String {
    "({ QUEUE_NAME, WorkerOptions, OtelCollector }) =>
  (
    processEventsFunction: WorkerProcessFunction<EncryptedEventEnvelope>,
    failureHandler: WorkerFailureHandler<EncryptedEventEnvelope>
  ) =>
    new KafkaWorkerConsumer(
      QUEUE_NAME,
      WorkerOptions,
      processEventsFunction,
      failureHandler,
      OtelCollector
    )".to_string()
}
fn get_redis_worker_consumer_factory(_pascal_case_name: &str) -> String {
    "({ TtlCache, QUEUE_NAME, WorkerOptions }) =>
  (
    processEventsFunction: WorkerProcessFunction<EncryptedEventEnvelope>,
    failureHandler: WorkerFailureHandler<EncryptedEventEnvelope>
  ) =>
    new RedisWorkerConsumer(
      QUEUE_NAME,
      TtlCache,
      WorkerOptions,
      processEventsFunction,
      failureHandler
    )".to_string()
}

pub(crate) fn get_worker_consumer_factory(r#type: &WorkerType, pascal_case_name: &str) -> String {
    match r#type {
        WorkerType::BullMQCache => get_bullmq_worker_consumer_factory(pascal_case_name),
        WorkerType::Database => get_database_worker_consumer_factory(pascal_case_name),
        WorkerType::Kafka => get_kafka_worker_consumer_factory(pascal_case_name),
        WorkerType::RedisCache => get_redis_worker_consumer_factory(pascal_case_name),
    }
}

const BULLMQ_WORKER_PRODUCER_FACTORY: &str = "({ QUEUE_NAME, WorkerOptions }) =>
   new BullMqWorkerProducer(QUEUE_NAME, WorkerOptions)";
const DATABASE_WORKER_PRODUCER_FACTORY: &str = "({ EntityMgr, WorkerOptions }) =>
  new DatabaseWorkerProducer(
    EntityMgr,
    WorkerOptions
  )";
const KAFKA_WORKER_PRODUCER_FACTORY: &str = "({ QUEUE_NAME, WorkerOptions }) =>
  new KafkaWorkerProducer(
    QUEUE_NAME,
    WorkerOptions
  )";
const REDIS_WORKER_PRODUCER_FACTORY: &str = "({ TtlCache, QUEUE_NAME, WorkerOptions }) =>
  new RedisWorkerProducer(
    QUEUE_NAME,
    TtlCache,
    WorkerOptions
  )";

pub(crate) fn get_worker_producer_factory(r#type: &WorkerType) -> String {
    match r#type {
        WorkerType::BullMQCache => BULLMQ_WORKER_PRODUCER_FACTORY.to_string(),
        WorkerType::Database => DATABASE_WORKER_PRODUCER_FACTORY.to_string(),
        WorkerType::Kafka => KAFKA_WORKER_PRODUCER_FACTORY.to_string(),
        WorkerType::RedisCache => REDIS_WORKER_PRODUCER_FACTORY.to_string(),
    }
}
