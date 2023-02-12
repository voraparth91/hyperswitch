mod transformers;

use std::fmt::Debug;
use common_utils::ext_traits::ValueExt;
use error_stack::{ResultExt, IntoReport};
use serde::{Deserialize, Serialize};

use crate::{
    configs::settings,
    utils::{self, BytesExt, OptionExt},
    core::{
        errors::{self, CustomResult},
        payments,
    },
    headers, logger, services::{self, ConnectorIntegration},
    types::{
        self,
        api::{self, ConnectorCommon, ConnectorCommonExt},
        ErrorResponse, Response, storage, ResponseId,
    }
};


use transformers as forte;

#[derive(Debug, Clone)]
pub struct Forte;

impl<Flow, Request, Response> ConnectorCommonExt<Flow, Request, Response> for Forte 
where
    Self: ConnectorIntegration<Flow, Request, Response>,{
    fn build_headers(
        &self,
        req: &types::RouterData<Flow, Request, Response>,
        _connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, String)>, errors::ConnectorError> {
        // println!("_req in hackathon:: {}", auth);
        let metadata = req
            .connector_meta_data
            .clone()
            .ok_or(errors::ConnectorError::RequestEncodingFailed)?;
        let connector_metadata: ConnectorMetadata = metadata
            .parse_value("ConnectorMetadata")
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        let org_value = format!("org_{}", connector_metadata.org_id);
        let mut header = vec![("X-Forte-Auth-Organization-Id".to_string(), org_value),
        ("Content-Type".to_string(),"application/json".to_string())];
        let mut api_key = self.get_auth_header(&req.connector_auth_type)?;
        header.append(&mut api_key);

        Ok(header)
    }
}

impl ConnectorCommon for Forte {
    fn id(&self) -> &'static str {
        "forte"
    }

    fn common_get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn base_url<'a>(&self, connectors: &'a settings::Connectors) -> &'a str {
        connectors.forte.base_url.as_ref()
    }

    fn get_auth_header(&self, auth_type:&types::ConnectorAuthType)-> CustomResult<Vec<(String,String)>,errors::ConnectorError> {
        let auth: forte::ForteAuthType = auth_type
            .try_into()
            .change_context(errors::ConnectorError::FailedToObtainAuthType)?;
        Ok(vec![(headers::AUTHORIZATION.to_string(), auth.api_key)])
    }

    fn build_error_response(
        &self,
        res: types::Response,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        logger::debug!(payu_error_response=?res);
        let response: forte::FortePaymentsResponse = res
            .response
            .parse_struct("Forte ErrorResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        Ok(ErrorResponse {
            status_code: res.status_code,
            code: response.response.response_code,
            message: response.response.response_desc,
            reason: None,
        })
    }
}

impl api::Payment for Forte {}

impl api::PreVerify for Forte {}
impl
    ConnectorIntegration<
        api::Verify,
        types::VerifyRequestData,
        types::PaymentsResponseData,
    > for Forte
{
}

impl api::PaymentVoid for Forte {}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ConnectorMetadata {
    org_id : String,
    location_id : String
}

impl
    ConnectorIntegration<
        api::Void,
        types::PaymentsCancelData,
        types::PaymentsResponseData,
    > for Forte
{
    fn get_headers(&self, req: &types::PaymentsCancelRouterData, connectors: &settings::Connectors,) -> CustomResult<Vec<(String, String)>,errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(&self, req: &types::PaymentsCancelRouterData, _connectors: &settings::Connectors,) -> CustomResult<String,errors::ConnectorError> {
        let transaction_id = &req.request.connector_transaction_id;
        let metadata = req
            .connector_meta_data
            .clone()
            .ok_or(errors::ConnectorError::RequestEncodingFailed)?;
        let connector_metadata: ConnectorMetadata = metadata
            .parse_value("ConnectorMetadata")
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(format!("{}organizations/org_{}/locations/loc_{}/transactions/{}", self.base_url(_connectors), connector_metadata.org_id, connector_metadata.location_id, transaction_id))
    }

    fn get_request_body(&self, req: &types::PaymentsCancelRouterData) -> CustomResult<Option<String>,errors::ConnectorError> {
        let forte_req =
            utils::Encode::<forte::ForteCancelRequest>::convert_and_encode(req).change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(Some(forte_req))
    }

    fn build_request(
        &self,
        req: &types::PaymentsCancelRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        Ok(Some(
            services::RequestBuilder::new()
                .method(services::Method::Put)
                .url(&types::PaymentsVoidType::get_url(self, req, connectors)?)
                .headers(types::PaymentsVoidType::get_headers(self, req, connectors)?)
                .body(types::PaymentsVoidType::get_request_body(self, req)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &types::PaymentsCancelRouterData,
        res: Response,
    ) -> CustomResult<types::PaymentsCancelRouterData,errors::ConnectorError> {
        let response: forte::FortePaymentsResponse = res.response.parse_struct("PaymentIntentResponse").change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
        logger::debug!(fortepayments_create_response=?response);
        types::ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        }
        .try_into()
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }

    fn get_error_response(&self, res: Response) -> CustomResult<ErrorResponse,errors::ConnectorError> {
        self.build_error_response(res)
    }
}

impl api::ConnectorAccessToken for Forte {}

impl ConnectorIntegration<api::AccessTokenAuth, types::AccessTokenRequestData, types::AccessToken>
    for Forte
{
}

impl api::PaymentSync for Forte {}
impl
    ConnectorIntegration<api::PSync, types::PaymentsSyncData, types::PaymentsResponseData>
    for Forte
{
    fn get_headers(
        &self,
        req: &types::PaymentsSyncRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, String)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(
        &self,
        req: &types::PaymentsSyncRouterData,
        _connectors: &settings::Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        let transaction_id = match &req.request.connector_transaction_id {
                ResponseId::ConnectorTransactionId(x) => x.to_string(),
                _ => "123".to_string()
            };
        let metadata = req
            .connector_meta_data
            .clone()
            .ok_or(errors::ConnectorError::RequestEncodingFailed)?;
        let connector_metadata: ConnectorMetadata = metadata
            .parse_value("ConnectorMetadata")
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(format!("{}organizations/org_{}/locations/loc_{}/transactions/{}", self.base_url(_connectors), connector_metadata.org_id, connector_metadata.location_id, transaction_id))
    }

    fn build_request(
        &self,
        req: &types::PaymentsSyncRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        Ok(Some(
            services::RequestBuilder::new()
                .method(services::Method::Get)
                .url(&types::PaymentsSyncType::get_url(self, req, connectors)?)
                .headers(types::PaymentsSyncType::get_headers(self, req, connectors)?)
                .build(),
        ))
    }

    fn get_error_response(
        &self,
        res: Response,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res)
    }

    fn handle_response(
        &self,
        data: &types::PaymentsSyncRouterData,
        res: Response,
    ) -> CustomResult<types::PaymentsSyncRouterData, errors::ConnectorError> {
        logger::debug!(payment_sync_response=?res);
        let response: forte:: FortePaymentsResponse = res
            .response
            .parse_struct("forte PaymentsResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
        types::RouterData::try_from(types::ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }
}


impl api::PaymentCapture for Forte {}
impl
    ConnectorIntegration<
        api::Capture,
        types::PaymentsCaptureData,
        types::PaymentsResponseData,
    > for Forte
{
    fn get_headers(
        &self,
        req: &types::PaymentsCaptureRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, String)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(
        &self,
        req: &types::PaymentsCaptureRouterData,
        _connectors: &settings::Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        let metadata = req
            .connector_meta_data
            .clone()
            .ok_or(errors::ConnectorError::RequestEncodingFailed)?;
        let connector_metadata: ConnectorMetadata = metadata
            .parse_value("ConnectorMetadata")
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(format!("{}organizations/org_{}/locations/loc_{}/transactions", self.base_url(_connectors), connector_metadata.org_id, connector_metadata.location_id))
    }

    fn get_request_body(
        &self,
        req: &types::PaymentsCaptureRouterData,
    ) -> CustomResult<Option<String>, errors::ConnectorError> {
        let forte_req =
            utils::Encode::<forte::ForteCaptureRequest>::convert_and_encode(req).change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(Some(forte_req))
    }

    fn build_request(
        &self,
        req: &types::PaymentsCaptureRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        Ok(Some(
            services::RequestBuilder::new()
                .method(services::Method::Put)
                .url(&types::PaymentsCaptureType::get_url(self, req, connectors)?)
                .headers(types::PaymentsCaptureType::get_headers(self, req, connectors,)?)
                .body(types::PaymentsCaptureType::get_request_body(self, req)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &types::PaymentsCaptureRouterData,
        res: Response,
    ) -> CustomResult<types::PaymentsCaptureRouterData, errors::ConnectorError> {
        let response: forte::FortePaymentsResponse = res
            .response
            .parse_struct("Forte PaymentsResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
        logger::debug!(fortepayments_create_response=?response);
        types::ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        }
        .try_into()
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }

    fn get_error_response(
        &self,
        res: Response,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res)
    }
}

impl api::PaymentSession for Forte {}

impl
    ConnectorIntegration<
        api::Session,
        types::PaymentsSessionData,
        types::PaymentsResponseData,
    > for Forte
{
    //TODO: implement sessions flow
}


impl api::PaymentAuthorize for Forte {}

impl
    ConnectorIntegration<
        api::Authorize,
        types::PaymentsAuthorizeData,
        types::PaymentsResponseData,
    > for Forte {
    fn get_headers(&self, req: &types::PaymentsAuthorizeRouterData, connectors: &settings::Connectors,) -> CustomResult<Vec<(String, String)>,errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(&self, req: &types::PaymentsAuthorizeRouterData, _connectors: &settings::Connectors,) -> CustomResult<String,errors::ConnectorError> {
        let metadata = req
            .connector_meta_data
            .clone()
            .ok_or(errors::ConnectorError::RequestEncodingFailed)?;
        let connector_metadata: ConnectorMetadata = metadata
            .parse_value("ConnectorMetadata")
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        match req.request.capture_method {
            Some(storage::enums::CaptureMethod::Automatic) => Ok(format!("{}organizations/org_{}/locations/loc_{}/transactions", self.base_url(_connectors), connector_metadata.org_id, connector_metadata.location_id)),
            _ => Ok(format!("{}organizations/org_{}/locations/loc_{}/transactions/authorize", self.base_url(_connectors), connector_metadata.org_id, connector_metadata.location_id))
        }
    }

    fn get_request_body(&self, req: &types::PaymentsAuthorizeRouterData) -> CustomResult<Option<String>,errors::ConnectorError> {
        let forte_req =
            utils::Encode::<forte::FortePaymentsRequest>::convert_and_encode(req).change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(Some(forte_req))
    }

    fn build_request(
        &self,
        req: &types::PaymentsAuthorizeRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        Ok(Some(
            services::RequestBuilder::new()
                .method(services::Method::Post)
                .url(&types::PaymentsAuthorizeType::get_url(
                    self, req, connectors,
                )?)
                .headers(types::PaymentsAuthorizeType::get_headers(
                    self, req, connectors,
                )?)
                .body(types::PaymentsAuthorizeType::get_request_body(self, req)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &types::PaymentsAuthorizeRouterData,
        res: Response,
    ) -> CustomResult<types::PaymentsAuthorizeRouterData,errors::ConnectorError> {
        let response: forte::FortePaymentsResponse = res.response.parse_struct("PaymentIntentResponse").change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
        logger::debug!(fortepayments_create_response=?response);
        types::ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        }
        .try_into()
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }

    fn get_error_response(&self, res: Response) -> CustomResult<ErrorResponse,errors::ConnectorError> {
        self.build_error_response(res)
    }
}

impl api::Refund for Forte {}
impl api::RefundExecute for Forte {}
impl api::RefundSync for Forte {}

impl
    ConnectorIntegration<
        api::Execute,
        types::RefundsData,
        types::RefundsResponseData,
    > for Forte {
    fn get_headers(&self, req: &types::RefundsRouterData<api::Execute>, connectors: &settings::Connectors,) -> CustomResult<Vec<(String,String)>,errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(&self, req: &types::RefundsRouterData<api::Execute>, _connectors: &settings::Connectors,) -> CustomResult<String,errors::ConnectorError> {
        let metadata = req
            .connector_meta_data
            .clone()
            .ok_or(errors::ConnectorError::RequestEncodingFailed)?;
        let connector_metadata: ConnectorMetadata = metadata
            .parse_value("ConnectorMetadata")
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(format!("{}organizations/org_{}/locations/loc_{}/transactions", self.base_url(_connectors), connector_metadata.org_id, connector_metadata.location_id))
    }

    fn get_request_body(&self, req: &types::RefundsRouterData<api::Execute>) -> CustomResult<Option<String>,errors::ConnectorError> {
        let forte_req = utils::Encode::<forte::ForteRefundRequest>::convert_and_encode(req).change_context(errors::ConnectorError::RequestEncodingFailed)?;
        Ok(Some(forte_req))
    }

    fn build_request(&self, req: &types::RefundsRouterData<api::Execute>, connectors: &settings::Connectors,) -> CustomResult<Option<services::Request>,errors::ConnectorError> {
        let request = services::RequestBuilder::new()
            .method(services::Method::Post)
            .url(&types::RefundExecuteType::get_url(self, req, connectors)?)
            .headers(types::RefundExecuteType::get_headers(self, req, connectors)?)
            .body(types::RefundExecuteType::get_request_body(self, req)?)
            .build();
        Ok(Some(request))
    }

    fn handle_response(
        &self,
        data: &types::RefundsRouterData<api::Execute>,
        res: Response,
    ) -> CustomResult<types::RefundsRouterData<api::Execute>,errors::ConnectorError> {
        logger::debug!(target: "router::connector::forte", response=?res);
        let response: forte::RefundResponse = res.response.parse_struct("forte RefundResponse").change_context(errors::ConnectorError::RequestEncodingFailed)?;
        types::ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        }
        .try_into()
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }

    fn get_error_response(&self, res: Response) -> CustomResult<ErrorResponse,errors::ConnectorError> {
        self.build_error_response(res)
    }
}

impl
    ConnectorIntegration<api::RSync, types::RefundsData, types::RefundsResponseData> for Forte {
    fn get_headers(&self, req: &types::RefundSyncRouterData,connectors: &settings::Connectors,) -> CustomResult<Vec<(String, String)>,errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(&self, req: &types::RefundSyncRouterData,_connectors: &settings::Connectors,) -> CustomResult<String,errors::ConnectorError> {
        let metadata = req
            .connector_meta_data
            .clone()
            .ok_or(errors::ConnectorError::RequestEncodingFailed)?;
        let connector_metadata: ConnectorMetadata = metadata
            .parse_value("ConnectorMetadata")
            .change_context(errors::ConnectorError::RequestEncodingFailed)?;
        let connector_refund_id = req.request.connector_refund_id.clone()
            .get_required_value("connector_refund_id")
            .change_context(errors::ConnectorError::MissingRequiredField { field_name: "connector_refund_id" })?; 
        Ok(format!("{}organizations/org_{}/locations/loc_{}/transactions/{}", self.base_url(_connectors), connector_metadata.org_id, connector_metadata.location_id, connector_refund_id))
    }

    fn build_request(
        &self,
        req: &types::RefundSyncRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        Ok(Some(
            services::RequestBuilder::new()
                .method(services::Method::Get)
                .url(&types::RefundSyncType::get_url(self, req, connectors)?)
                .headers(types::RefundSyncType::get_headers(self, req, connectors)?)
                .body(types::RefundSyncType::get_request_body(self, req)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &types::RefundSyncRouterData,
        res: Response,
    ) -> CustomResult<types::RefundSyncRouterData,errors::ConnectorError,> {
        logger::debug!(target: "router::connector::forte", response=?res);
        let response: forte::RefundResponse = res.response.parse_struct("forte RefundResponse").change_context(errors::ConnectorError::ResponseDeserializationFailed)?;
        types::ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        }
        .try_into()
        .change_context(errors::ConnectorError::ResponseHandlingFailed)
    }

    fn get_error_response(&self, res: Response) -> CustomResult<ErrorResponse,errors::ConnectorError> {
        logger::debug!(payu_error_response=?res);
        let response: forte::FortePaymentsResponse = res
            .response
            .parse_struct("Forte ErrorResponse")
            .change_context(errors::ConnectorError::ResponseDeserializationFailed)?;

        Ok(ErrorResponse {
            status_code: res.status_code,
            code: response.response.response_code,
            message: response.response.response_desc,
            reason: None,
        })
    }
}

#[async_trait::async_trait]
impl api::IncomingWebhook for Forte {
    fn get_webhook_object_reference_id(
        &self,
        _body: &[u8],
    ) -> CustomResult<String, errors::ConnectorError> {
        Err(errors::ConnectorError::WebhooksNotImplemented).into_report()
    }

    fn get_webhook_event_type(
        &self,
        _body: &[u8],
    ) -> CustomResult<api::IncomingWebhookEvent, errors::ConnectorError> {
        Err(errors::ConnectorError::WebhooksNotImplemented).into_report()
    }

    fn get_webhook_resource_object(
        &self,
        _body: &[u8],
    ) -> CustomResult<serde_json::Value, errors::ConnectorError> {
        Err(errors::ConnectorError::WebhooksNotImplemented).into_report()
    }
}

impl services::ConnectorRedirectResponse for Forte {
    fn get_flow_type(
        &self,
        _query_params: &str,
    ) -> CustomResult<payments::CallConnectorAction, errors::ConnectorError> {
        Ok(payments::CallConnectorAction::Trigger)
    }
}
