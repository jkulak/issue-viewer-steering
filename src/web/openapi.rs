//! The published description of the routes, written to `openapi.json` by `make openapi`.
//!
//! The pages are HTML and the forms are what a browser sends, so the schema says which addresses
//! there are and which fields each form takes; a test checks that every route listed here answers.

use serde_json::{Map, Value, json};

pub struct Field {
    pub name: &'static str,
    pub kind: &'static str,
    pub list: bool,
    pub description: &'static str,
}

const fn field(name: &'static str, kind: &'static str, description: &'static str) -> Field {
    Field {
        name,
        kind,
        list: false,
        description,
    }
}

pub enum Answer {
    Html,
    Json(&'static str),
    Redirect,
}

pub struct Route {
    pub method: &'static str,
    pub path: &'static str,
    pub summary: &'static str,
    pub query: &'static [Field],
    pub form: &'static [Field],
    pub answer: Answer,
}

const VERSION_FIELD: Field = field(
    "version",
    "string",
    "The version of the file the edit is based on",
);
const PROJECTS_FIELD: Field = Field {
    name: "project",
    kind: "string",
    list: true,
    description: "The projects to show; every project when there is none",
};
const FILTER_FIELD: Field = field(
    "q",
    "string",
    "Filter the cards like the issue list: label:bug, is:open, words",
);
const DONE_FIELD: Field = field(
    "done",
    "string",
    "all to show every card in Done, not only the ten most recent",
);

pub const ROUTES: &[Route] = &[
    Route {
        method: "get",
        path: "/",
        summary: "The projects",
        query: &[],
        form: &[],
        answer: Answer::Html,
    },
    Route {
        method: "get",
        path: "/-/open",
        summary: "Browse the folders under the open root",
        query: &[field(
            "path",
            "string",
            "A folder relative to the open root",
        )],
        form: &[],
        answer: Answer::Html,
    },
    Route {
        method: "post",
        path: "/-/open",
        summary: "Open a folder under the open root as a project",
        query: &[],
        form: &[field(
            "path",
            "string",
            "A folder relative to the open root",
        )],
        answer: Answer::Redirect,
    },
    Route {
        method: "get",
        path: "/-/board",
        summary: "The board of several projects, or of all of them",
        query: &[PROJECTS_FIELD, FILTER_FIELD, DONE_FIELD],
        form: &[],
        answer: Answer::Html,
    },
    Route {
        method: "get",
        path: "/-/board/version",
        summary: "What an open board polls to notice a change to an issue file or a branch",
        query: &[PROJECTS_FIELD],
        form: &[],
        answer: Answer::Json("{version}"),
    },
    Route {
        method: "get",
        path: "/{project_name}/issues",
        summary: "The issue list",
        query: &[
            field(
                "q",
                "string",
                "A search like GitHub's: is:open, label:bug, sort:updated-desc, words",
            ),
            field("page", "integer", "The page, from 1"),
        ],
        form: &[],
        answer: Answer::Html,
    },
    Route {
        method: "post",
        path: "/{project_name}/issues",
        summary: "Create an issue",
        query: &[],
        form: &[
            field("title", "string", "The title"),
            field("description", "string", "The description, in Markdown"),
            Field {
                name: "labels",
                kind: "string",
                list: true,
                description: "The labels",
            },
        ],
        answer: Answer::Redirect,
    },
    Route {
        method: "get",
        path: "/{project_name}/issues/new",
        summary: "The form for a new issue",
        query: &[],
        form: &[],
        answer: Answer::Html,
    },
    Route {
        method: "get",
        path: "/{project_name}/issues/{number}",
        summary: "An issue",
        query: &[],
        form: &[],
        answer: Answer::Html,
    },
    Route {
        method: "post",
        path: "/{project_name}/issues/{number}/title",
        summary: "Change the title",
        query: &[],
        form: &[field("title", "string", "The new title")],
        answer: Answer::Redirect,
    },
    Route {
        method: "post",
        path: "/{project_name}/issues/{number}/description",
        summary: "Change the description",
        query: &[],
        form: &[
            field("text", "string", "The new description, in Markdown"),
            VERSION_FIELD,
        ],
        answer: Answer::Redirect,
    },
    Route {
        method: "post",
        path: "/{project_name}/issues/{number}/labels",
        summary: "Set the labels",
        query: &[],
        form: &[Field {
            name: "labels",
            kind: "string",
            list: true,
            description: "The labels",
        }],
        answer: Answer::Redirect,
    },
    Route {
        method: "post",
        path: "/{project_name}/issues/{number}/comments",
        summary: "Comment, close or reopen",
        query: &[],
        form: &[
            field(
                "text",
                "string",
                "The comment, in Markdown; may be empty when closing or reopening",
            ),
            field(
                "action",
                "string",
                "comment, close, close-not-planned or reopen",
            ),
        ],
        answer: Answer::Redirect,
    },
    Route {
        method: "post",
        path: "/{project_name}/issues/{number}/comments/{index}",
        summary: "Change a comment",
        query: &[],
        form: &[
            field("text", "string", "The new comment, in Markdown"),
            VERSION_FIELD,
        ],
        answer: Answer::Redirect,
    },
    Route {
        method: "post",
        path: "/{project_name}/issues/{number}/comments/{index}/delete",
        summary: "Delete a comment",
        query: &[],
        form: &[VERSION_FIELD],
        answer: Answer::Redirect,
    },
    Route {
        method: "post",
        path: "/{project_name}/issues/{number}/tasks",
        summary: "Tick or untick a checkbox",
        query: &[],
        form: &[
            field("part", "string", "description, or the index of a comment"),
            field("line", "integer", "The line of the checkbox in that part"),
            field(
                "checked",
                "string",
                "1, true or on to tick it; anything else unticks it",
            ),
            VERSION_FIELD,
        ],
        answer: Answer::Json("The new version and task counts: {version, done, total}"),
    },
    Route {
        method: "post",
        path: "/{project_name}/issues/{number}/status",
        summary: "Move the issue to a column of the board; with Accept: application/json the answer \
                  is {version, column} instead of a redirect",
        query: &[],
        form: &[
            field(
                "status",
                "string",
                "backlog, selected, in-progress, testing, review, blocked or done",
            ),
            VERSION_FIELD,
            field(
                "back",
                "string",
                "The page to go back to, a path on this site; the board when missing",
            ),
        ],
        answer: Answer::Redirect,
    },
    Route {
        method: "get",
        path: "/{project_name}/labels",
        summary: "The labels",
        query: &[],
        form: &[],
        answer: Answer::Html,
    },
    Route {
        method: "get",
        path: "/{project_name}/board",
        summary: "The project's board",
        query: &[FILTER_FIELD, DONE_FIELD],
        form: &[],
        answer: Answer::Html,
    },
    Route {
        method: "get",
        path: "/{project_name}/-/version",
        summary: "What open pages poll to notice a change on disk",
        query: &[field(
            "issue",
            "integer",
            "An issue number; without it, the version of the whole project",
        )],
        form: &[],
        answer: Answer::Json("{version}: a string, or null for an issue that is not there"),
    },
    Route {
        method: "post",
        path: "/{project_name}/preview",
        summary: "Render Markdown the way an issue shows it",
        query: &[],
        form: &[field("text", "string", "Markdown")],
        answer: Answer::Json("{html}"),
    },
];

fn property(field: &Field) -> Value {
    let item = json!({ "type": field.kind, "description": field.description });
    if field.list {
        json!({ "type": "array", "items": { "type": field.kind }, "description": field.description })
    } else {
        item
    }
}

fn operation(route: &Route) -> Value {
    let mut parameters: Vec<Value> = route
        .path
        .split('/')
        .filter_map(|part| part.strip_prefix('{')?.strip_suffix('}'))
        .map(|name| {
            let kind = if name == "project_name" {
                "string"
            } else {
                "integer"
            };
            json!({ "name": name, "in": "path", "required": true, "schema": { "type": kind } })
        })
        .collect();
    parameters.extend(route.query.iter().map(|field| {
        json!({ "name": field.name, "in": "query", "required": false, "schema": property(field) })
    }));
    let mut operation = Map::new();
    operation.insert("summary".into(), json!(route.summary));
    if !parameters.is_empty() {
        operation.insert("parameters".into(), json!(parameters));
    }
    if route.method == "post" {
        let mut properties = Map::new();
        properties.insert(
            "_csrf".into(),
            json!({ "type": "string", "description": "The token the page embeds; or send it as X-CSRF-Token" }),
        );
        for field in route.form {
            properties.insert(field.name.into(), property(field));
        }
        operation.insert(
            "requestBody".into(),
            json!({ "content": { "application/x-www-form-urlencoded": { "schema": { "type": "object", "properties": properties } } } }),
        );
    }
    let success = match route.answer {
        Answer::Html => {
            json!({ "200": { "description": "An HTML page", "content": { "text/html": {} } } })
        }
        Answer::Json(description) => {
            json!({ "200": { "description": description, "content": { "application/json": {} } } })
        }
        Answer::Redirect => {
            json!({ "303": { "description": "Saved; the Location header is the page to show next" } })
        }
    };
    let mut responses = success.as_object().cloned().unwrap_or_default();
    responses.insert(
        "default".into(),
        json!({ "description": "An error page, or {error} with Accept: application/json" }),
    );
    operation.insert("responses".into(), Value::Object(responses));
    Value::Object(operation)
}

pub fn schema() -> Value {
    let mut paths = Map::new();
    for route in ROUTES {
        let entry = paths.entry(route.path).or_insert_with(|| json!({}));
        entry[route.method] = operation(route);
    }
    json!({
        "openapi": "3.1.0",
        "info": { "title": "Issue viewer", "version": env!("CARGO_PKG_VERSION") },
        "paths": paths,
    })
}
