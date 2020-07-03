/*!
 * Experimental "high-level" dropshot interface.  The idea behind this interface
 * is to allow consumers to define a type of resource (like, say, a "Project"),
 * and the essential operations on it (like listing by name, or by id) and let
 * Dropshot handle the boilerplate associated with all the individual route
 * handlers.
 *
 * For example, we might like to say that Projects:
 *
 * - can be rendered to the client using a custom "view" type
 * - can be created with custom "create params" type
 * - can be looked up by name OR id
 * - can be listed by name OR id
 * - can be deleted by name OR id
 * - can be updated by name OR id via PUT (replace) with custom "replace params"
 *   type
 *
 * With today's "low level" interface, you can define a bunch of API handler
 * functions to carry out these operations, but there are a few downsides to
 * this:
 *
 * - There's a bunch of boilerplate that's replicated for each set of handlers.
 * - There's nothing to guarantee that boilerplate is correct / consistent.  For
 *   example, the list operations should all look the same using the same names
 *   for, say, the "limit" parameter.  But there's nothing to enforce that.
 * - It would be really nice if Dropshot could implement conditional requests.
 *   This feels possible if consumers could tell Dropshot how to get the ETag of
 *   a resource.  But this has a huge downside -- see below.
 * - It would be really nice if the consumer could implement "update" once and
 *   Dropshot could use that to support both PUT (replace) and PATCH (update).
 *   But it's not yet clear how this interface would look.
 *
 * Ideally, the consumer could just define the pieces it needs for a specific
 * resource like "Project".  You could imagine an interface where the consumer
 * specifies:
 *
 * - a function to create a client view of a Project
 * - a function to look up a Project by name
 * - a function to look up a Project by id
 *
 * This allows Dropshot to implement GET by looking up a project and converting
 * it to its view.  We could add:
 *
 * - a function to get the ETag of a Project
 *
 * This allows Dropshot to implement conditional GET requests, too.  Great!  We
 * might also add:
 *
 * - a function to create a Project from the create params
 * - a function to update (replace) a Project (NOTE: not by id or name, but
 *   using the result of a previous lookup) using "update params"
 * - a function to delete a Project (same)
 *
 * With this interface, Dropshot could implement POST to create projects as well
 * as DELETE and PUT, including conditional requests.  Also great!  But there
 * are a few big problems.
 *
 * Before we get to the problems, we should also mention that the consumer would
 * also need a way to specify where this resource gets "mounted" into the
 * namespace.  It's even possible that it would go in two places, or that one of
 * those places would be a filtered view of the resource (e.g., listing
 * instances in a project shows a different thing than listing instances on a
 * server).  One idea would be that the traits through which consumers implement
 * these functions also provide a function that returns a RouteHandler and the
 * consumer has complete control over where that goes in the URL namespace.
 *
 *
 * THE PROBLEM WITH IMPLEMENTING CONDITIONAL REQUESTS IN DROPSHOT
 *
 * There's one big problem with this approach: every "update" and "delete"
 * require a previous "lookup", even when the operation is unconditional.  In a
 * practical system, this often means every database "write" operation becomes a
 * "database read + "database write" operation.  This can have a huge impact on
 * scalability of write-heavy workloads.  For those cases, a consumer probably
 * wants to implement the conditional handling itself.  If the request is
 * conditional, you could imagine a SQL query like this:
 *
 *     DELETE FROM Projects WHERE id = 123 AND ETag = "abcdefg12345";
 *
 * (You'd want to do something a little more sophisticated to distinguish the
 * "no such project" case from the "project matched, but had a different ETag
 * case", but it still seems like this could be possible with a single database
 * transaction.)
 *
 * If consumers want to do this, there doesn't seem to be any way to implement
 * any of the conditional request behavior inside Dropshot, not even a
 * validation that the consumer did the right thing.  (That is, it would be easy
 * to build a consumer that ignored the ETag and did the wrong thing.)
 *
 *
 * WHAT ABOUT UPDATES: REPLACE vs. PATCH
 *
 * There are at least two ways to update a resource in HTTP: PUT (which is
 * supposed to replace the entire thing) or PATCH (for which different formats
 * exist that describe how to modify the existing thing).  It would be nice if
 * the consumer could implement one form of update and Dropshot could implement:
 *
 * - update-as-complete-replacement
 * - update-using-JSON-Patch
 * - update-using-JSON-Merge-Patch
 *
 * This seems somewhat hard because consumers provide the View and UpdateParams
 * as arbitrary serde-supporting types.  Maybe we could do this by:
 *
 * - consumers implement update-as-complete-replacement
 * - to implement JSON Patch, Dropshot:
 *   - fetches the resource (including etag)
 *   - serializes the view to JSON
 *   - applies the JSON patch to this JSON representation
 *   - attempts to deserialize the patched resource into the UpdateParams type
 *   - if that succeeds, use the update-as-complete-replacement function
 * - to implement JSON Merge Patch, Dropshot does something similar.  The only
 *   different step is the application of the client's input to the JSON
 *   representation of the resource.
 *
 * If we go this route, then there's now a distinction between the HTTP requests
 * handled by the server and the calls to the client's functions.  This might
 * matter a lot for metrics, logging, etc.  We'd want to think about how to
 * clearly expose this as an interface.
 */

use crate::HttpError;
use crate::RequestContext;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;

pub type HttpResult<T> = Result<T, HttpError>;

/** an HTTP ETag (typically a string identifying the content of a resource) */
/* TODO-cleanup should split out for input/output */
/*
 * TODO even better would be to let consumers provide their own struct that we
 * will checksum or the like so they don't all have to parse the string.
 */
pub enum ETag {
    /** matches all etags */
    Any,
    /** matches a specific etag */
    ETagValue(String),
}

/** describes preconditions for the request */
pub enum Condition {
    /** execute only if the resource currently matches the etag */
    IfMatchETag(ETag),
    /** execute only if the resource currently doesn't match the etag */
    IfNotMatchETag(ETag),
}

/**
 * Top-level trait for all "high level" resources.
 */
pub trait Resource: Sized {
    type View: Serialize;
    /** Returns the client view of a resource. */
    fn as_view(&self) -> Self::View;
    /** Returns the ETag for a resource */
    fn etag(&self) -> ETag;
}

/**
 * Implement this to support creation of a resource.  (For example, this might
 * insert the resource into a database.)
 */
pub trait Create: Resource {
    type CreateParams: DeserializeOwned;
    fn create(
        rqctx: Arc<RequestContext>,
        params: Self::CreateParams,
    ) -> HttpResult<Self>;
}

/**
 * Implement this to support GET of an object in a collection based on marker
 * fields `ByKey`.
 */
pub trait Lookup<ByKey>: Resource
where
    ByKey: DeserializeOwned,
{
    fn lookup(rqctx: Arc<RequestContext>, key: ByKey) -> HttpResult<Self>;
}

#[derive(Deserialize, Serialize)]
#[serde(rename = "lowercase")]
pub enum PaginationOrder {
    Ascending,
    Descending,
}

#[derive(Deserialize, Serialize)]
#[serde(rename = "lowercase")]
enum MarkerVersion {
    V1,
}

#[derive(Deserialize, Serialize)]
pub struct Marker<MarkerFields> {
    dropshot_marker_version: MarkerVersion,
    order: PaginationOrder,
    pub page_start: MarkerFields,
}

#[derive(Deserialize, Serialize)]
pub struct PaginationParams<MarkerFields> {
    limit: Option<u32>,
    marker: Option<Marker<MarkerFields>>,
    order: Option<PaginationOrder>,
}

/**
 * Implement this to support listing a collection of this resource, paginated
 * using marker fields `ByKey`.
 */
pub trait List<ByKey>: Resource
where
    ByKey: DeserializeOwned,
{
    fn list(
        rqctx: Arc<RequestContext>,
        pag_params: PaginationParams<ByKey>,
    ) -> HttpResult<Vec<Self>>;
}

/**
 * Implement this to support DELETE that replaces an entire object.
 */
pub trait DeleteUnconditional<ByKey>: Resource
where
    ByKey: DeserializeOwned,
{
    fn delete_unconditional(
        rqctx: Arc<RequestContext>,
        key: ByKey,
    ) -> HttpResult<()>;
}

/**
 * Implement this to support PUT that replaces an entire object.
 */
pub trait UpdateReplaceUnconditional<ByKey>: Resource
where
    ByKey: DeserializeOwned,
{
    type UpdateReplaceParams: DeserializeOwned;

    fn update_replace(
        rqctx: Arc<RequestContext>,
        key: ByKey,
        params: Self::UpdateReplaceParams,
    ) -> HttpResult<Self>;
}

pub trait DeleteConditional<ByKey>: Resource
where
    ByKey: DeserializeOwned,
{
    fn delete_conditional(
        rqctx: Arc<RequestContext>,
        key: ByKey,
        cond: Condition,
    ) -> HttpResult<()>;
}
