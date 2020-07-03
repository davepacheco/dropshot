// Copyright 2020 Oxide Computer Company
/*!
 * Exercise high-level interface
 */
#![allow(unused_variables)]

use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;
use dropshot::RequestContext;
use dropshot::highlevel::HttpResult;
use dropshot::highlevel::ETag;
use dropshot::highlevel::Condition;
use dropshot::highlevel::Resource;
use dropshot::highlevel::Create;
use dropshot::highlevel::Lookup;
use dropshot::highlevel::PaginationParams;
use dropshot::highlevel::List;
use dropshot::highlevel::DeleteUnconditional;
use dropshot::highlevel::UpdateReplaceUnconditional;
use dropshot::highlevel::DeleteConditional;

/* resource-agnostic types */

#[derive(Clone, Deserialize, Serialize)]
struct Name(String); /* XXX comes from elsewhere */

#[derive(Deserialize, Serialize)]
struct ByName {
    name: Name,
}

#[derive(Deserialize, Serialize)]
struct ById {
    id: Uuid,
}

/*
 * Hypothetical API - data model
 */

#[derive(Deserialize, Serialize)]
struct Project {
    id: Uuid,
    name: Name,
    description: String,
    generation: u32,
}

#[derive(Deserialize, Serialize)]
struct ProjectView {
    id: Uuid,
    name: Name,
    description: String,
    generation: u32,
}

#[derive(Deserialize, Serialize)]
struct ProjectCreateParams {
    name: Name,
    description: String,
    generation: u32,
}

#[derive(Deserialize, Serialize)]
struct ProjectReplaceParams {
    name: Name,
    description: String,
    generation: u32,
}

/*
 * Hypothetical API - implementation
 */

impl Resource for Project {
    type View = ProjectView;

    fn as_view(&self) -> Self::View {
        ProjectView {
            id: self.id,
            name: self.name.clone(),
            description: self.description.clone(),
            generation: self.generation,
        }
    }

    fn etag(&self) -> ETag {
        ETag::ETagValue(format!("{}-{}", self.id, self.generation))
    }
}

impl Create for Project {
    type CreateParams = ProjectCreateParams;

    fn create(
        rqctx: Arc<RequestContext>,
        params: ProjectCreateParams,
    ) -> HttpResult<Project> {
        Ok(Project {
            id: Uuid::new_v4(),
            name: params.name.clone(),
            description: params.description.clone(),
            generation: 1,
        })
    }
}

impl Lookup<ByName> for Project {
    fn lookup(rqctx: Arc<RequestContext>, key: ByName) -> HttpResult<Self> {
        unimplemented!(); // TODO
    }
}

impl Lookup<ById> for Project {
    fn lookup(rqctx: Arc<RequestContext>, key: ById) -> HttpResult<Self> {
        unimplemented!(); // TODO
    }
}

impl List<ByName> for Project {
    fn list(
        rqctx: Arc<RequestContext>,
        pag_params: PaginationParams<ByName>,
    ) -> HttpResult<Vec<Self>> {
        unimplemented!(); // TODO
    }
}

impl List<ById> for Project {
    fn list(
        rqctx: Arc<RequestContext>,
        pag_params: PaginationParams<ById>,
    ) -> HttpResult<Vec<Self>> {
        unimplemented!(); // TODO
    }
}

impl DeleteUnconditional<ById> for Project {
    fn delete_unconditional(
        rqctx: Arc<RequestContext>,
        key: ById,
    ) -> HttpResult<()> {
        unimplemented!(); // TODO
    }
}

impl DeleteUnconditional<ByName> for Project {
    fn delete_unconditional(
        rqctx: Arc<RequestContext>,
        key: ByName,
    ) -> HttpResult<()> {
        unimplemented!(); // TODO
    }
}

impl DeleteConditional<ById> for Project {
    fn delete_conditional(
        rqctx: Arc<RequestContext>,
        key: ById,
        cond: Condition,
    ) -> HttpResult<()> {
        unimplemented!(); // TODO
    }
}

impl DeleteConditional<ByName> for Project {
    fn delete_conditional(
        rqctx: Arc<RequestContext>,
        key: ByName,
        cond: Condition,
    ) -> HttpResult<()> {
        unimplemented!(); // TODO
    }
}

impl UpdateReplaceUnconditional<ById> for Project {
    type UpdateReplaceParams = ProjectReplaceParams;

    fn update_replace(
        rqctx: Arc<RequestContext>,
        key: ById,
        params: ProjectReplaceParams,
    ) -> HttpResult<Self> {
        unimplemented!(); // TODO
    }
}

impl UpdateReplaceUnconditional<ByName> for Project {
    type UpdateReplaceParams = ProjectReplaceParams;

    fn update_replace(
        rqctx: Arc<RequestContext>,
        key: ByName,
        params: ProjectReplaceParams,
    ) -> HttpResult<Self> {
        unimplemented!(); // TODO
    }
}
