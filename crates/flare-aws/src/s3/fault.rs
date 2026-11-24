//! The fault that tests inject into a file put inside the SDK: the answer
//! of the put's first attempt is replaced with a 500, so the SDK's own
//! retry sends the put again. Without the fault a put is sent unchanged.

use crate::dynamodb::Faults;
use aws_sdk_s3::client::customize::CustomizableOperation;
use aws_sdk_s3::config::interceptors::BeforeDeserializationInterceptorContextMut;
use aws_sdk_s3::config::{ConfigBag, Intercept, RuntimeComponents};
use aws_sdk_s3::operation::put_object::PutObjectError;
use aws_sdk_s3::operation::put_object::PutObjectOutput;
use aws_sdk_s3::operation::put_object::builders::PutObjectFluentBuilder;
use std::sync::atomic::{AtomicBool, Ordering};

/// A put ready to send.
pub(crate) type Put =
    CustomizableOperation<PutObjectOutput, PutObjectError, PutObjectFluentBuilder>;

/// `request` with `faults` applied.
pub(crate) fn prepare(request: PutObjectFluentBuilder, faults: Faults) -> Put {
    let put = request.customize();
    if faults == Faults::FailFirstPutAttempts {
        put.interceptor(FirstAttemptFails::default())
    } else {
        put
    }
}

/// Replaces the status of the first answer it reads with 500. One belongs
/// to one put, and the put's retries read their answers unchanged.
#[derive(Debug, Default)]
struct FirstAttemptFails {
    done: AtomicBool,
}

impl Intercept for FirstAttemptFails {
    fn name(&self) -> &'static str {
        "FirstAttemptFails"
    }

    fn modify_before_deserialization(
        &self,
        context: &mut BeforeDeserializationInterceptorContextMut<'_>,
        _components: &RuntimeComponents,
        _cfg: &mut ConfigBag,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.done.swap(true, Ordering::SeqCst) {
            *context.response_mut().status_mut() = 500_u16.try_into()?;
        }
        Ok(())
    }
}
