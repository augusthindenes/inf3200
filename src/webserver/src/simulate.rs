use actix_web::{
    body::EitherBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpResponse,
};
use std::future::{ready, Ready};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared state to track whether the node is "crashed" or operational
pub struct CrashState {
    crashed: AtomicBool,
}

impl CrashState {
    pub fn new() -> Self {
        CrashState {
            crashed: AtomicBool::new(false),
        }
    }

    /// Simulate a crash, this will cause all non-crash/recover endpoints to return 503
    pub fn crash(&self) {
        self.crashed.store(true, Ordering::Relaxed);
        println!("Node simulating crash - responses disabled");
    }

    /// Recover from a simulated crash - resume normal operations
    pub fn recover(&self) {
        self.crashed.store(false, Ordering::Relaxed);
        println!("Node recovered from simulated crash - responses enabled");
    }

    /// Check if the node is currently in a crashed state
    pub fn is_crashed(&self) -> bool {
        self.crashed.load(Ordering::Relaxed)
    }
}

impl Default for CrashState {
    fn default() -> Self {
        Self::new()
    }
}

/// Middleware factory for crash simulation
/// This middleware intercepts requests and returns 503 Service Unavailable
/// when the node is in a "crashed" state, except for crash/recover endpoints
pub struct CrashSimulator {
    state: Arc<CrashState>,
}

impl CrashSimulator {
    pub fn new(state: Arc<CrashState>) -> Self {
        CrashSimulator { state }
    }
}

impl<S, B> Transform<S, ServiceRequest> for CrashSimulator
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = CrashSimulatorMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(CrashSimulatorMiddleware {
            service,
            state: Arc::clone(&self.state),
        }))
    }
}

/// The actual middleware that intercepts requests
pub struct CrashSimulatorMiddleware<S> {
    service: S,
    state: Arc<CrashState>,
}

impl<S, B> Service<ServiceRequest> for CrashSimulatorMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let path = req.path().to_string();
        let is_crashed = self.state.is_crashed();
        
        // Allow crash and recover endpoints to always work
        let is_control_endpoint = path == "/sim-crash" || path == "/sim-recover";
        
        // If crashed and not a control endpoint, return 503
        if is_crashed && !is_control_endpoint {
            return Box::pin(async move {
                let (req, _) = req.into_parts();
                let response = HttpResponse::ServiceUnavailable()
                    .body("Node is currently in simulated crash state");
                
                let srv_response = ServiceResponse::new(req, response);
                Ok(srv_response.map_into_right_body())
            });
        }
        
        // Otherwise, proceed normally
        let fut = self.service.call(req);
        Box::pin(async move {
            let res = fut.await?;
            Ok(res.map_into_left_body())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crash_state() {
        let state = CrashState::new();
        assert!(!state.is_crashed());

        state.crash();
        assert!(state.is_crashed());

        state.recover();
        assert!(!state.is_crashed());
    }

}
