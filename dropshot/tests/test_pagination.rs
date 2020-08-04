// Copyright 2020 Oxide Computer Company
/*!
 * Test cases for API handler functions that use pagination.
 */

use dropshot::endpoint;
use dropshot::test_util::object_get;
use dropshot::test_util::objects_list_page;
use dropshot::test_util::ClientTestContext;
use dropshot::ApiDescription;
use dropshot::EmptyScanParams;
use dropshot::ExtractedParameter;
use dropshot::HttpError;
use dropshot::HttpResponseOkObject;
use dropshot::PaginationParams;
use dropshot::Query;
use dropshot::RequestContext;
use dropshot::ResultsPage;
use dropshot::WhichPage;
use http::Method;
use http::StatusCode;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::ops::Range;
use std::sync::atomic::AtomicU16;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[macro_use]
extern crate slog;

mod common;

/*
 * Common helpers
 */

/**
 * Given a test context and URL path, assert that a GET request to that path
 * (with an empty body) produces a 400 response with the given error message.
 */
async fn assert_error(
    client: &ClientTestContext,
    path: &str,
    expected_message: &str,
) {
    let error = client
        .make_request_error(Method::GET, path, StatusCode::BAD_REQUEST)
        .await;
    assert_eq!(error.message, expected_message,);
    assert_eq!(error.error_code, None);
}

/**
 * Given an array of integers, check that they're sequential starting at
 * "offset".
 */
fn assert_sequence_from(items: &Vec<u16>, offset: u16, count: u16) {
    let nchecked = AtomicU16::new(0);
    items.iter().enumerate().for_each(|(i, c)| {
        assert_eq!(*c, (i as u16) + offset);
        nchecked.fetch_add(1, Ordering::SeqCst);
    });
    assert_eq!(nchecked.load(Ordering::SeqCst) as usize, items.len());
    assert_eq!(count as usize, items.len());
}

/**
 * Page selector for a set of "u16" values
 *
 * This is used in several tests below.
 */
#[derive(Debug, Deserialize, ExtractedParameter, Serialize)]
struct IntegersPageSelector {
    last_seen: u16,
}

fn page_selector_for(n: &u16, _p: &EmptyScanParams) -> IntegersPageSelector {
    IntegersPageSelector {
        last_seen: *n,
    }
}

/**
 * Define an API with a couple of different endpoints that allow us to exercise
 * various functionality.
 */
fn paginate_api() -> ApiDescription {
    let mut api = ApiDescription::new();
    api.register(api_integers).unwrap();
    api.register(api_empty).unwrap();
    api.register(api_with_extra_params).unwrap();
    api
}

/*
 * Basic tests
 */

/**
 * "/intapi": a collection of positive values of "u16" (excepting u16::MAX).
 * The marker is simply the last number seen.
 */
#[endpoint {
    method = GET,
    path = "/intapi",
}]
async fn api_integers(
    rqctx: Arc<RequestContext>,
    query: Query<PaginationParams<EmptyScanParams, IntegersPageSelector>>,
) -> Result<HttpResponseOkObject<ResultsPage<u16>>, HttpError> {
    let pag_params = query.into_inner();
    let limit = rqctx.page_limit(&pag_params)?.get() as u16;

    let start = match &pag_params.page {
        WhichPage::First(..) => 0,
        WhichPage::Next(IntegersPageSelector {
            last_seen,
        }) => *last_seen,
    };

    let results = if start < std::u16::MAX {
        let start = start + 1;
        let end = start.checked_add(limit).unwrap_or(std::u16::MAX);
        (start..end).collect()
    } else {
        Vec::new()
    };

    Ok(HttpResponseOkObject(ResultsPage::new(
        results,
        &EmptyScanParams {},
        page_selector_for,
    )?))
}

#[tokio::test]
async fn test_paginate_errors() {
    let api = paginate_api();
    let testctx = common::test_setup("paginate_errors", api);
    let client = &testctx.client_testctx;

    struct ErrorTestCase {
        path: String,
        message: &'static str,
    };
    let test_cases = vec![
        ErrorTestCase {
            path: "/intapi?limit=0".to_string(),
            message: "unable to parse query string: expected a non-zero value",
        },
        ErrorTestCase {
            path: "/intapi?limit=-3".to_string(),
            message: "unable to parse query string: invalid digit found in \
                      string",
        },
        ErrorTestCase {
            path: "/intapi?limit=seven".to_string(),
            message: "unable to parse query string: invalid digit found in \
                      string",
        },
        ErrorTestCase {
            path: format!("/intapi?limit={}", (std::u64::MAX as u128) + 1),
            message: "unable to parse query string: number too large to fit \
                      in target type",
        },
        ErrorTestCase {
            path: "/intapi?page_token=q".to_string(),
            message: "unable to parse query string: failed to parse \
                      pagination token: Encoded text cannot have a 6-bit \
                      remainder.",
        },
    ];

    for tc in test_cases {
        assert_error(client, &tc.path, tc.message).await;
    }
}

#[tokio::test]
async fn test_paginate_basic() {
    let api = paginate_api();
    let testctx = common::test_setup("paginate_basic", api);
    let client = &testctx.client_testctx;

    /*
     * "First page" test cases
     */

    /*
     * Test the default value of "limit".  This test will have to be updated if
     * we change the default count of items, but it's important to check that
     * the default actually works and is reasonable.
     */
    let expected_default = 100;
    let page = objects_list_page::<u16>(&client, "/intapi").await;
    assert_sequence_from(&page.items, 1, expected_default);
    assert!(page.next_page.is_some());

    /*
     * Test the maximum value of "limit" by providing a value much higher than
     * we support and observing it get clamped.  As with the previous test, this
     * will have to be updated if we change the maximum count, but it's worth it
     * to test this case.
     */
    let expected_max = 10000;
    let page = objects_list_page::<u16>(
        &client,
        &format!("/intapi?limit={}", 2 * expected_max),
    )
    .await;
    assert_sequence_from(&page.items, 1, expected_max);

    /*
     * Limits in between the default and the max should also work.  This
     * exercises the `page_limit()` function.
     */
    let count = 2 * expected_default;
    assert!(count > expected_default);
    assert!(count < expected_max);
    let page =
        objects_list_page::<u16>(&client, &format!("/intapi?limit={}", count))
            .await;
    assert_sequence_from(&page.items, 1, count);

    /*
     * "Next page" test cases
     */

    /*
     * Run the same few limit tests as above.
     */
    let next_page_start = page.items.last().unwrap() + 1;
    let next_page_token = page.next_page.unwrap();

    let page = objects_list_page::<u16>(
        &client,
        &format!("/intapi?page_token={}", next_page_token,),
    )
    .await;
    assert_sequence_from(&page.items, next_page_start, expected_default);
    assert!(page.next_page.is_some());

    let page = objects_list_page::<u16>(
        &client,
        &format!(
            "/intapi?page_token={}&limit={}",
            next_page_token,
            2 * expected_max
        ),
    )
    .await;
    assert_sequence_from(&page.items, next_page_start, expected_max);
    assert!(page.next_page.is_some());

    let page = objects_list_page::<u16>(
        &client,
        &format!("/intapi?page_token={}&limit={}", next_page_token, count),
    )
    .await;
    assert_sequence_from(&page.items, next_page_start, count);
    assert!(page.next_page.is_some());

    /*
     * Loop through the entire collection.
     */
    let mut next_item = 1u16;
    let mut page = objects_list_page::<u16>(
        &client,
        &format!("/intapi?limit={}", expected_max),
    )
    .await;
    loop {
        if let Some(ref next_token) = page.next_page {
            if page.items.len() != expected_max as usize {
                assert!(page.items.len() > 0);
                assert!(page.items.len() < expected_max as usize);
                assert_eq!(*page.items.last().unwrap(), std::u16::MAX - 1);
            }
            assert_sequence_from(
                &page.items,
                next_item,
                page.items.len() as u16,
            );
            next_item += page.items.len() as u16;
            page = objects_list_page::<u16>(
                &client,
                &format!(
                    "/intapi?page_token={}&limit={}",
                    &next_token, expected_max
                ),
            )
            .await;
        } else {
            assert_eq!(page.items.len(), 0);
            break;
        }
    }

    testctx.teardown().await;
}

/*
 * Tests for an empty collection
 */

/**
 * "/empty": an empty collection of u16s, useful for testing the case where the
 * first request in a scan returns no results.
 */
#[endpoint {
    method = GET,
    path = "/empty",
}]
async fn api_empty(
    _rqctx: Arc<RequestContext>,
    _query: Query<PaginationParams<EmptyScanParams, IntegersPageSelector>>,
) -> Result<HttpResponseOkObject<ResultsPage<u16>>, HttpError> {
    Ok(HttpResponseOkObject(ResultsPage::new(
        Vec::new(),
        &EmptyScanParams {},
        page_selector_for,
    )?))
}

/*
 * Tests various cases related to an empty collection, particularly making sure
 * that basic parsing of query parameters still does what we expect and that we
 * get a valid results page with no objects.
 */
#[tokio::test]
async fn test_paginate_empty() {
    let api = paginate_api();
    let testctx = common::test_setup("paginate_empty", api);
    let client = &testctx.client_testctx;

    let page = objects_list_page::<u16>(&client, "/empty").await;
    assert_eq!(page.items.len(), 0);
    assert!(page.next_page.is_none());

    let page = objects_list_page::<u16>(&client, "/empty?limit=10").await;
    assert_eq!(page.items.len(), 0);
    assert!(page.next_page.is_none());

    assert_error(
        &client,
        "/empty?limit=0",
        "unable to parse query string: expected a non-zero value",
    ).await;

    assert_error(
        &client,
        "/empty?page_token=q",
        "unable to parse query string: failed to parse pagination token: \
         Encoded text cannot have a 6-bit remainder.",
    ).await;

    testctx.teardown().await;
}

/*
 * Test extra query parameters and response properties
 */

/**
 * "/ints_extra": also a paginated collection of "u16" values.  This
 * API exercises consuming additional query parameters ("debug") and sending a
 * more complex response type.
 */

#[endpoint {
    method = GET,
    path = "/ints_extra",
}]
async fn api_with_extra_params(
    rqctx: Arc<RequestContext>,
    query_pag: Query<PaginationParams<EmptyScanParams, IntegersPageSelector>>,
    query_extra: Query<ExtraQueryParams>,
) -> Result<HttpResponseOkObject<ExtraResultsPage>, HttpError> {
    let pag_params = query_pag.into_inner();
    let limit = rqctx.page_limit(&pag_params)?.get() as u16;
    let extra_params = query_extra.into_inner();

    /* XXX see previous function */
    let start = match &pag_params.page {
        WhichPage::First(..) => 0,
        WhichPage::Next(IntegersPageSelector {
            last_seen,
        }) => *last_seen,
    };

    let results = Range {
        start: start + 1,
        end: start + limit + 1,
    }
    .collect();

    Ok(HttpResponseOkObject(ExtraResultsPage {
        debug_was_set: extra_params.debug.is_some(),
        debug_value: extra_params.debug.unwrap_or(false),
        page: ResultsPage::new(
            results,
            &EmptyScanParams {},
            page_selector_for,
        )?,
    }))
}

/* TODO-coverage check generated OpenAPI spec */
#[derive(Deserialize, ExtractedParameter)]
struct ExtraQueryParams {
    debug: Option<bool>,
}

/* TODO-coverage check generated OpenAPI spec */
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct ExtraResultsPage {
    debug_was_set: bool,
    debug_value: bool,
    #[serde(flatten)]
    page: ResultsPage<u16>,
}

#[tokio::test]
async fn test_paginate_extra_params() {
    let api = paginate_api();
    let testctx = common::test_setup("paginate_extra_params", api);
    let client = &testctx.client_testctx;

    /* Test that the extra query parameter is optional. */
    let page =
        object_get::<ExtraResultsPage>(&client, "/ints_extra?limit=5").await;
    assert!(!page.debug_was_set);
    assert!(!page.debug_value);
    assert_eq!(page.page.items, vec![1, 2, 3, 4, 5]);
    let token = page.page.next_page.unwrap();

    /* Provide a value for the extra query parameter in the FirstPage case. */
    let page = object_get::<ExtraResultsPage>(
        &client,
        "/ints_extra?limit=5&debug=true",
    )
    .await;
    assert!(page.debug_was_set);
    assert!(page.debug_value);
    assert_eq!(page.page.items, vec![1, 2, 3, 4, 5]);
    assert!(page.page.next_page.is_some());

    /* Provide a value for the extra query parameter in the NextPage case. */
    let page = object_get::<ExtraResultsPage>(
        &client,
        &format!("/ints_extra?page_token={}&debug=false&limit=7", token),
    )
    .await;
    assert_eq!(page.page.items, vec![6, 7, 8, 9, 10, 11, 12]);
    assert!(page.debug_was_set);
    assert!(!page.debug_value);
    assert!(page.page.next_page.is_some());

    testctx.teardown().await;
}
