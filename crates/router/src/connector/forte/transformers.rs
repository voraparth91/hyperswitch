use common_utils::ext_traits::ValueExt;
use error_stack::{IntoReport, ResultExt};
use serde::{Deserialize, Serialize};
use crate::{
    connector::utils::{self, AddressDetailsData, PaymentsRequestData},
    core::errors,
    pii::PeekInterface,
    types::{self, api, storage::{enums, self}}, 
};

//TODO: Fill the struct with respective fields
#[derive(Default, Debug, Serialize, Eq, PartialEq)]
pub struct FortePaymentsRequest {
    action: String,
    authorization_amount: i64,
    billing_address: BillingAddress,
    card: CardDetails
}

#[derive(Default, Debug, Serialize, Eq, PartialEq)]
pub struct BillingAddress {
    first_name: String,
    last_name: String,
}

#[derive(Default, Debug, Serialize, Eq, PartialEq)]
pub struct CardDetails {
    card_type: String,
    name_on_card: String,
    account_number: String,
    expire_month: String,
    expire_year: String,
    card_verification_value: String,
}

impl TryFrom<&types::PaymentsAuthorizeRouterData> for FortePaymentsRequest  {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &types::PaymentsAuthorizeRouterData) -> Result<Self,Self::Error> {
        let todo_action = match item.request.capture_method {
            Some(storage::enums::CaptureMethod::Automatic) => "sale",
            _ => "authorize"
        };
        match item.request.payment_method_data {
            api::PaymentMethod::Card(ref ccard) => {
                let action = todo_action.to_string();
                let authorization_amount = item.request.amount;
                let address_details = item.get_billing()?
                    .address
                    .as_ref()
                    .ok_or_else(utils::missing_field_err("billing.address"))?;
                let billing_address = BillingAddress {
                    first_name: address_details.get_first_name()?.to_owned().peek().to_string(),
                    last_name: address_details.get_last_name()?.to_owned().peek().to_string(),
                };
                let card= CardDetails {
                    card_type: String::from("visa"),
                    name_on_card: ccard.card_holder_name.peek().clone(),
                    account_number: ccard.card_number.peek().clone(),
                    expire_month: ccard.card_exp_month.peek().clone(),
                    expire_year: ccard.card_exp_year.peek().clone(),
                    card_verification_value: ccard.card_cvc.peek().clone()
                };
                Ok(Self {
                    action,
                    authorization_amount,
                    billing_address,
                    card,
                })
            }
            _ => Err(errors::ConnectorError::NotImplemented("Payment methods".to_string()).into()),
        }
    }
}

// Auth Struct
pub struct ForteAuthType {
    pub(super) api_key: String
}

impl TryFrom<&types::ConnectorAuthType> for ForteAuthType  {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(auth_type: &types::ConnectorAuthType) -> Result<Self, Self::Error> {
        if let types::ConnectorAuthType::HeaderKey { api_key } = auth_type {
            Ok(Self {
                api_key: api_key.to_string(),
            })
        } else {
            Err(errors::ConnectorError::FailedToObtainAuthType.into())
        }
    }
}

// PaymentsResponse
//TODO: Append the remaining status flags
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum FortePaymentStatus {
    A,
    D,
    #[default]
    E
}

impl From<FortePaymentStatus> for enums::AttemptStatus {
    fn from(item: FortePaymentStatus) -> Self {
        match item {
            FortePaymentStatus::A => Self::Charged,
            FortePaymentStatus::D => Self::Failure,
            FortePaymentStatus::E => Self::Failure,
        }
    }
}

//TODO: Fill the struct with respective fields
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FortePaymentsResponse {
    transaction_id: String,
    pub response: ResponseDetails,
    authorization_code: Option<String>,
    authorization_amount: Option<f64>,
    action: String
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseDetails {
    pub response_type: FortePaymentStatus,
    pub response_desc: String,
    pub response_code: String
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaymentMetadata {
    pub authorization_code: String
}

pub fn convert_status(item: FortePaymentStatus, action: String) -> enums::AttemptStatus {
    if action == "sale" {
        match item {
            FortePaymentStatus::A => enums::AttemptStatus::Charged,
            FortePaymentStatus::D => enums::AttemptStatus::Failure,
            FortePaymentStatus::E => enums::AttemptStatus::Failure,
        }
    }
    else if action == "authorize" {
        match item {
            FortePaymentStatus::A => enums::AttemptStatus::Authorized,
            FortePaymentStatus::D => enums::AttemptStatus::Failure,
            FortePaymentStatus::E => enums::AttemptStatus::AuthorizationFailed,
        }
    }
    else if action == "void" {
        match item {
            FortePaymentStatus::A => enums::AttemptStatus::Voided,
            FortePaymentStatus::D => enums::AttemptStatus::Failure,
            FortePaymentStatus::E => enums::AttemptStatus::Failure,
        }
    }
    else if action == "capture" {
        match item {
            FortePaymentStatus::A => enums::AttemptStatus::Charged,
            FortePaymentStatus::D => enums::AttemptStatus::AuthorizationFailed,
            FortePaymentStatus::E => enums::AttemptStatus::Failure,
        }
    }
    else {
        match item {
            FortePaymentStatus::A => enums::AttemptStatus::Charged,
            FortePaymentStatus::D => enums::AttemptStatus::Failure,
            FortePaymentStatus::E => enums::AttemptStatus::Failure,
        }
    }
}

impl<F,T> TryFrom<types::ResponseRouterData<F, FortePaymentsResponse, T, types::PaymentsResponseData>> for types::RouterData<F, T, types::PaymentsResponseData> {
    type Error = error_stack::Report<errors::ParsingError>;
    fn try_from(item: types::ResponseRouterData<F, FortePaymentsResponse, T, types::PaymentsResponseData>) -> Result<Self,Self::Error> {
        Ok(Self {
            status: convert_status(item.response.response.response_type, item.response.action),
            response: Ok(types::PaymentsResponseData::TransactionResponse {
                resource_id: types::ResponseId::ConnectorTransactionId(item.response.transaction_id),
                redirection_data: None,
                redirect: false,
                mandate_reference: None,
                connector_metadata: match item.response.authorization_code {
                    Some(x) => Some(
                        serde_json::to_value( PaymentMetadata {
                            authorization_code: x
                        })
                        .into_report()
                        .change_context(errors::ParsingError)?,
                    ),
                    None => None
                }
            }),
            amount_captured: match item.response.authorization_amount {
                Some(x) => Some(x as i64),
                None => None
            },
            ..item.data
        })
    }
}
#[derive(Default, Debug, Serialize)]
pub struct ForteCancelRequest {
    action: String,
    authorization_code: String,
    entered_by: String
}

impl TryFrom<&types::PaymentsCancelRouterData> for ForteCancelRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &types::PaymentsCancelRouterData) -> Result<Self, Self::Error> {
        let metadata = item.request.connector_metadata
            .clone()
            .ok_or(errors::ConnectorError::RequestEncodingFailed)?;
        let payment_metadata: PaymentMetadata = metadata
            .parse_value("PaymentMetadata")
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(Self {
            action: String::from("void"),
            authorization_code: payment_metadata.authorization_code,
            entered_by: String::from("aditya")
        })
    }
}

#[derive(Default, Debug, Serialize)]
pub struct ForteCaptureRequest {
    action: String,
    authorization_code: String,
    transaction_id: String,
    authorization_amount: i64
}

impl TryFrom<&types::PaymentsCaptureRouterData> for ForteCaptureRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &types::PaymentsCaptureRouterData) -> Result<Self, Self::Error> {
        let authorization_amount = match item.request.amount_to_capture {
            Some(x) => x,
            _ => 0
        };
        let transaction_id = item.request.connector_transaction_id.clone();
        let metadata = item.request.connector_metadata
            .clone()
            .ok_or(errors::ConnectorError::RequestEncodingFailed)?;
        let payment_metadata: PaymentMetadata = metadata
            .parse_value("PaymentMetadata")
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(Self {
            action: String::from("capture"),
            authorization_code: payment_metadata.authorization_code,
            authorization_amount,
            transaction_id
        })
    }
}

//TODO: Fill the struct with respective fields
// REFUND :
// Type definition for RefundRequest
#[derive(Default, Debug, Serialize)]
pub struct ForteRefundRequest {
    action: String,
    authorization_amount: i64,
    original_transaction_id: String,
    authorization_code: String
}

impl<F> TryFrom<&types::RefundsRouterData<F>> for ForteRefundRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &types::RefundsRouterData<F>) -> Result<Self,Self::Error> {
        let action = "reverse".to_string();
        let authorization_amount = item.request.amount;
        let original_transaction_id = item.request.connector_transaction_id.clone();
        let metadata = item.request.connector_metadata
            .clone()
            .ok_or(errors::ConnectorError::RequestEncodingFailed)?;
        let payment_metadata: PaymentMetadata = metadata
            .parse_value("PaymentMetadata")
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        let authorization_code = payment_metadata.authorization_code;

        Ok(Self {
            action,
            authorization_amount,
            original_transaction_id,
            authorization_code
        })
    }
}

// Type definition for Refund Response
//TODO: Fill the struct with respective fields
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RefundResponse {
    transaction_id: String,
    response: ResponseDetails,
    authorization_code: String,
    action: String 
}

impl TryFrom<types::RefundsResponseRouterData<api::Execute, RefundResponse>>
    for types::RefundsRouterData<api::Execute>
{
    type Error = error_stack::Report<errors::ParsingError>;
    fn try_from(
        item: types::RefundsResponseRouterData<api::Execute, RefundResponse>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            response: Ok(types::RefundsResponseData {
                connector_refund_id: item.response.transaction_id,
                refund_status: match item.response.response.response_type {
                    FortePaymentStatus::A => enums::RefundStatus::Success,
                    FortePaymentStatus::D => enums::RefundStatus::Failure,
                    FortePaymentStatus::E => enums::RefundStatus::Failure,
                }
            }),
            ..item.data
        })
    }
}

impl TryFrom<types::RefundsResponseRouterData<api::RSync, RefundResponse>> for types::RefundsRouterData<api::RSync>
{
     type Error = error_stack::Report<errors::ParsingError>;
    fn try_from(_item: types::RefundsResponseRouterData<api::RSync, RefundResponse>) -> Result<Self,Self::Error> {
        println!("Parth4");
         todo!()
     }
 }

//TODO: Fill the struct with respective fields
#[derive(Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct ForteErrorResponse {}
