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
use dropshot::PaginationOrder;
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
use std::collections::BTreeSet;
use std::ops::Bound;
use std::sync::atomic::AtomicU16;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[macro_use]
extern crate slog;
#[macro_use]
extern crate lazy_static;

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
    api.register(api_dictionary).unwrap();
    api
}

fn range_u16(start: u16, limit: u16) -> Vec<u16> {
    if start < std::u16::MAX {
        let start = start + 1;
        let end = start.checked_add(limit).unwrap_or(std::u16::MAX);
        (start..end).collect()
    } else {
        Vec::new()
    }
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

    Ok(HttpResponseOkObject(ResultsPage::new(
        range_u16(start, limit),
        &EmptyScanParams {},
        page_selector_for,
    )?))
}

#[tokio::test]
async fn test_paginate_errors() {
    let api = paginate_api();
    let testctx = common::test_setup("errors", api);
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
    let testctx = common::test_setup("basic", api);
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
    let testctx = common::test_setup("empty", api);
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
    )
    .await;

    assert_error(
        &client,
        "/empty?page_token=q",
        "unable to parse query string: failed to parse pagination token: \
         Encoded text cannot have a 6-bit remainder.",
    )
    .await;

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

    let start = match &pag_params.page {
        WhichPage::First(..) => 0,
        WhichPage::Next(IntegersPageSelector {
            last_seen,
        }) => *last_seen,
    };

    Ok(HttpResponseOkObject(ExtraResultsPage {
        debug_was_set: extra_params.debug.is_some(),
        debug_value: extra_params.debug.unwrap_or(false),
        page: ResultsPage::new(
            range_u16(start, limit),
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
    let testctx = common::test_setup("extra_params", api);
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

/*
 * Test an endpoint with scan options that returns custom structures.  Our
 * endpoint will return a list of words, with the marker being the last word
 * seen.
 */

lazy_static! {
    static ref WORD_LIST: BTreeSet<String> = make_word_list();
}

fn make_word_list() -> BTreeSet<String> {
    let word_list = include_str!("wordlist.txt");
    word_list.lines().map(|s| s.to_string()).collect()
}

/*
 * The use of a structure here is kind of pointless except to exercise the case
 * of endpoints that return a custom structure.
 */
#[derive(Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
struct DictionaryWord {
    word: String,
    length: usize,
}

#[derive(Clone, Deserialize, ExtractedParameter, Serialize)]
struct DictionaryScanParams {
    #[serde(default = "ascending")]
    order: PaginationOrder,
    #[serde(default)]
    /* Work around serde-rs/serde#1183 */
    #[serde(with = "serde_with::rust::display_fromstr")]
    min_length: usize,
}

fn ascending() -> PaginationOrder {
    PaginationOrder::Ascending
}

#[derive(Deserialize, Serialize)]
struct DictionaryPageSelector {
    scan: DictionaryScanParams,
    last_seen: String,
}

#[endpoint {
    method = GET,
    path = "/dictionary",
}]
async fn api_dictionary(
    rqctx: Arc<RequestContext>,
    query: Query<
        PaginationParams<DictionaryScanParams, DictionaryPageSelector>,
    >,
) -> Result<HttpResponseOkObject<ResultsPage<DictionaryWord>>, HttpError> {
    let pag_params = query.into_inner();
    let limit = rqctx.page_limit(&pag_params)?.get();
    let dictionary: &BTreeSet<String> = &*WORD_LIST;

    let (bound, scan_params) = match &pag_params.page {
        WhichPage::First(scan) => (Bound::Unbounded, scan),
        WhichPage::Next(DictionaryPageSelector {
            scan,
            last_seen,
        }) => (Bound::Excluded(last_seen), scan),
    };

    let (range_bounds, reverse) = match scan_params.order {
        PaginationOrder::Ascending => ((bound, Bound::Unbounded), true),
        PaginationOrder::Descending => ((Bound::Unbounded, bound), false),
    };

    let iter = dictionary.range::<String, _>(range_bounds);
    let iter: dyn Iterator<Item = &String> =
        if reverse { iter } else { iter.rev() };
    let iter = iter.filter_map(|word| {
        if word.len() >= scan_params.min_length {
            Some(DictionaryWord {
                word: word.clone(),
                length: word.len(),
            })
        } else {
            None
        }
    });

    Ok(HttpResponseOkObject(ResultsPage::new(
        iter.take(limit).collect(),
        scan_params,
        |item: &DictionaryWord, scan_params: &DictionaryScanParams| {
            DictionaryPageSelector {
                scan: scan_params.clone(),
                last_seen: item.word.clone(),
            }
        },
    )?))
}

#[tokio::test]
async fn test_paginate_dictionary() {
    let api = paginate_api();
    let testctx = common::test_setup("dictionary", api);
    let client = &testctx.client_testctx;

    /* simple case */
    let page =
        objects_list_page::<DictionaryWord>(&client, "/dictionary?limit=3")
            .await;
    assert_eq!(page.items, vec![
        DictionaryWord {
            word: "A&M".to_string(),
            length: 3
        },
        DictionaryWord {
            word: "A&P".to_string(),
            length: 3
        },
        DictionaryWord {
            word: "AAA".to_string(),
            length: 3
        },
    ]);
    let token = page.next_page.unwrap();
    let page = objects_list_page::<DictionaryWord>(
        &client,
        &format!("/dictionary?limit=3&page_token={}", token),
    )
    .await;
    assert_eq!(page.items, vec![
        DictionaryWord {
            word: "AAAS".to_string(),
            length: 4
        },
        DictionaryWord {
            word: "ABA".to_string(),
            length: 3
        },
        DictionaryWord {
            word: "AC".to_string(),
            length: 2
        },
    ]);

    /* Reverse the order. */
    let page = objects_list_page::<DictionaryWord>(
        &client,
        "/dictionary?limit=3&order=descending",
    )
    .await;
    assert_eq!(page.items, vec![
        DictionaryWord {
            word: "zygote".to_string(),
            length: 6
        },
        DictionaryWord {
            word: "zucchini".to_string(),
            length: 8
        },
        DictionaryWord {
            word: "zounds".to_string(),
            length: 6
        },
    ]);
    let token = page.next_page.unwrap();
    /* Critically, we don't have to pass order=descending again. */
    let page = objects_list_page::<DictionaryWord>(
        &client,
        &format!("/dictionary?limit=3&page_token={}", token),
    )
    .await;
    assert_eq!(page.items, vec![
        DictionaryWord {
            word: "zooplankton".to_string(),
            length: 11
        },
        DictionaryWord {
            word: "zoom".to_string(),
            length: 4
        },
        DictionaryWord {
            word: "zoology".to_string(),
            length: 7
        },
    ]);

    /* Apply a filter. */
    let page = objects_list_page::<DictionaryWord>(
        &client,
        "/dictionary?limit=3&min_length=12",
    )
    .await;
    let found_words =
        page.items.iter().map(|dw| dw.word.as_str()).collect::<Vec<&str>>();
    assert_eq!(found_words, vec![
        "Addressograph",
        "Aristotelean",
        "Aristotelian",
    ]);
    let token = page.next_page.unwrap();
    let page = objects_list_page::<DictionaryWord>(
        &client,
        &format!("/dictionary?limit=3&page_token={}", token),
    )
    .await;
    assert_eq!(page.items, vec![
        DictionaryWord {
            word: "Bhagavadgita".to_string(),
            length: 12
        },
        DictionaryWord {
            word: "Brontosaurus".to_string(),
            length: 12
        },
        DictionaryWord {
            word: "Cantabrigian".to_string(),
            length: 12
        },
    ]);
}
