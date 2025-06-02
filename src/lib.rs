pub mod llmproxy {
    tonic::include_proto!("llmproxy");
}

pub mod aimodels;
pub mod communication;
pub mod database;
pub mod mq;
pub mod routes;
pub mod trading;
