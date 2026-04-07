// MCP Server - Exposes Project Cartographer via Model Context Protocol
// This allows AI tools and agents to interact with Cartographer using MCP

use crate::api::{ApiState, ModuleContextRequest, ProjectGraphResponse};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

macro_rules! mcprop {
    ($type:literal, $desc:literal) => {
        McpProperty {
            type_: $type.to_string(),
            description: $desc.to_string(),
        }
    };
}

macro_rules! mcinput {
    ($($key:literal => $type:literal => $desc:literal),* $(,)?) => {{
        let mut props = HashMap::new();
        $(
            props.insert($key.to_string(), mcprop!($type, $desc));
        )*
        McpInputSchema {
            type_: "object".to_string(),
            properties: props,
            required: vec![$($key.to_string()),*],
        }
    }};
}

/// MCP Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: McpInputSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInputSchema {
    pub type_: String,
    pub properties: HashMap<String, McpProperty>,
    pub required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpProperty {
    pub type_: String,
    pub description: String,
}

/// MCP Resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: Option<String>,
}

/// MCP Prompt definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    pub name: String,
    pub description: String,
    pub arguments: Vec<McpArgument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpArgument {
    pub name: String,
    pub description: String,
    pub required: bool,
}

/// MCP Server capabilities
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpCapabilities {
    pub tools: bool,
    pub resources: bool,
    pub prompts: bool,
}

/// MCP Server info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
    pub capabilities: McpCapabilities,
}

impl Default for McpServerInfo {
    fn default() -> Self {
        Self {
            name: "Project Cartographer MCP Server".to_string(),
            version: "1.0.0".to_string(),
            capabilities: McpCapabilities {
                tools: true,
                resources: true,
                prompts: true,
            },
        }
    }
}

/// MCP Tool Call request
#[derive(Debug, Deserialize)]
pub struct McpToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// MCP Tool Call response
#[derive(Debug, Serialize)]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    pub is_error: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum McpContent {
    Text { text: String },
    Image { data: String, mime_type: String },
    Resource { resource: McpResource },
}

impl McpContent {
    pub fn text(content: String) -> Self {
        McpContent::Text { text: content }
    }
}

/// MCP Server implementation
pub struct McpServer {
    api_state: std::sync::Arc<ApiState>,
    tools: Vec<McpTool>,
    resources: Vec<McpResource>,
    prompts: Vec<McpPrompt>,
}

impl McpServer {
    pub fn new(api_state: std::sync::Arc<ApiState>) -> Self {
        let tools = Self::create_tools();
        let resources = Self::create_resources();
        let prompts = Self::create_prompts();

        Self {
            api_state,
            tools,
            resources,
            prompts,
        }
    }

    fn create_tools() -> Vec<McpTool> {
        vec![
            McpTool {
                name: "get_module_context".to_string(),
                description:
                    "Get the public API surface of a specific module with optional dependencies"
                        .to_string(),
                input_schema: McpInputSchema {
                    type_: "object".to_string(),
                    properties: {
                        let mut props = HashMap::new();
                        props.insert(
                            "module_id".to_string(),
                            McpProperty {
                                type_: "string".to_string(),
                                description:
                                    "Unique identifier for the module (file path or module name)"
                                        .to_string(),
                            },
                        );
                        props.insert(
                            "depth".to_string(),
                            McpProperty {
                                type_: "number".to_string(),
                                description: "Depth of transitive dependencies (0 = module only)"
                                    .to_string(),
                            },
                        );
                        props.insert(
                            "detail_level".to_string(),
                            mcprop!("string", "Level of detail: minimal, standard, extended"),
                        );
                        props
                    },
                    required: vec!["module_id".to_string()],
                },
            },
            McpTool {
                name: "get_symbol_context".to_string(),
                description: "Get context for a specific symbol within a module".to_string(),
                input_schema: mcinput!(
                    "module_id" => "string" => "Module containing the symbol",
                    "symbol_name" => "string" => "Name of the symbol to retrieve",
                    "detail_level" => "string" => "Level of detail: minimal, standard, extended"
                ),
            },
            McpTool {
                name: "get_project_graph".to_string(),
                description: "Get the full project dependency graph".to_string(),
                input_schema: McpInputSchema {
                    type_: "object".to_string(),
                    properties: HashMap::new(),
                    required: vec![],
                },
            },
            McpTool {
                name: "get_dependencies".to_string(),
                description: "Get direct/transitive dependencies of a module".to_string(),
                input_schema: mcinput!(
                    "module_id" => "string" => "Module to get dependencies for",
                    "depth" => "number" => "Dependency depth (default 1)"
                ),
            },
            McpTool {
                name: "get_dependents".to_string(),
                description: "Get modules that depend on a given module".to_string(),
                input_schema: mcinput!(
                    "module_id" => "string" => "Module to get dependents for"
                ),
            },
            McpTool {
                name: "search_project".to_string(),
                description: "Search for modules matching a pattern".to_string(),
                input_schema: mcinput!(
                    "query" => "string" => "Search pattern",
                    "query_type" => "string" => "Type: node or edge"
                ),
            },
            McpTool {
                name: "get_blast_radius".to_string(),
                description: "Get related files/symbols for understanding change impact"
                    .to_string(),
                input_schema: mcinput!(
                    "target" => "string" => "File path or symbol name",
                    "max_related" => "number" => "Maximum related items (default 10)"
                ),
            },
            McpTool {
                name: "set_compression_level".to_string(),
                description: "Configure compression level for responses".to_string(),
                input_schema: mcinput!(
                    "level" => "string" => "Compression level: minimal, standard, aggressive"
                ),
            },
        ]
    }

    fn create_resources() -> Vec<McpResource> {
        vec![
            McpResource {
                uri: "cartographer://project-graph".to_string(),
                name: "project_graph".to_string(),
                description: "Full project dependency graph in JSON format".to_string(),
                mime_type: Some("application/json".to_string()),
            },
            McpResource {
                uri: "cartographer://module-index".to_string(),
                name: "module_index".to_string(),
                description: "Index of all mapped modules with their signatures".to_string(),
                mime_type: Some("application/json".to_string()),
            },
        ]
    }

    fn create_prompts() -> Vec<McpPrompt> {
        vec![
            McpPrompt {
                name: "analyze_module".to_string(),
                description: "Generate a prompt for analyzing a specific module".to_string(),
                arguments: vec![McpArgument {
                    name: "module_id".to_string(),
                    description: "Module to analyze".to_string(),
                    required: true,
                }],
            },
            McpPrompt {
                name: "plan_refactoring".to_string(),
                description: "Generate a prompt for planning refactoring of a module".to_string(),
                arguments: vec![
                    McpArgument {
                        name: "module_id".to_string(),
                        description: "Module to refactor".to_string(),
                        required: true,
                    },
                    McpArgument {
                        name: "goal".to_string(),
                        description: "Refactoring goal".to_string(),
                        required: true,
                    },
                ],
            },
        ]
    }

    pub fn get_server_info(&self) -> McpServerInfo {
        McpServerInfo::default()
    }

    pub fn list_tools(&self) -> Vec<McpTool> {
        self.tools.clone()
    }

    pub fn list_resources(&self) -> Vec<McpResource> {
        self.resources.clone()
    }

    pub fn list_prompts(&self) -> Vec<McpPrompt> {
        self.prompts.clone()
    }

    pub fn call_tool(&self, call: McpToolCall) -> Result<McpToolResult, String> {
        match call.name.as_str() {
            "get_module_context" => {
                let args = call.arguments;
                let request = ModuleContextRequest {
                    module_id: args
                        .get("module_id")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing module_id")?
                        .to_string(),
                    depth: args.get("depth").and_then(|v| v.as_u64()).map(|v| v as u32),
                    detail_level: args
                        .get("detail_level")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    include: None,
                    format: None,
                };

                let response = self.api_state.get_module_context(&request)?;
                Ok(McpToolResult {
                    content: vec![McpContent::text(
                        serde_json::to_string_pretty(&response).unwrap_or_default(),
                    )],
                    is_error: None,
                })
            }

            "get_project_graph" => {
                let graph = self.api_state.rebuild_graph()?;
                Ok(McpToolResult {
                    content: vec![McpContent::text(
                        serde_json::to_string_pretty(&graph).unwrap_or_default(),
                    )],
                    is_error: None,
                })
            }

            "get_dependencies" => {
                let args = call.arguments;
                let module_id = args
                    .get("module_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing module_id")?;
                let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

                let deps = self
                    .api_state
                    .get_dependencies_internal(module_id, depth)?
                    .ok_or("No dependencies found")?;

                Ok(McpToolResult {
                    content: vec![McpContent::text(
                        serde_json::to_string_pretty(&deps).unwrap_or_default(),
                    )],
                    is_error: None,
                })
            }

            "get_dependents" => {
                let args = call.arguments;
                let module_id = args
                    .get("module_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing module_id")?;

                let dependents = self.api_state.get_dependents(module_id)?;

                Ok(McpToolResult {
                    content: vec![McpContent::text(
                        serde_json::to_string_pretty(&dependents).unwrap_or_default(),
                    )],
                    is_error: None,
                })
            }

            "search_project" => {
                let args = call.arguments;
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing query")?;
                let query_type = args.get("query_type").and_then(|v| v.as_str());

                let results = self.api_state.search_graph(query, query_type)?;

                Ok(McpToolResult {
                    content: vec![McpContent::text(
                        serde_json::to_string_pretty(&results).unwrap_or_default(),
                    )],
                    is_error: None,
                })
            }

            "set_compression_level" => {
                let args = call.arguments;
                let level = args
                    .get("level")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing level")?;

                let level = match level {
                    "minimal" => crate::api::CompressionLevel::Minimal,
                    "aggressive" => crate::api::CompressionLevel::Aggressive,
                    _ => crate::api::CompressionLevel::Standard,
                };

                self.api_state.set_compression_level(level);

                Ok(McpToolResult {
                    content: vec![McpContent::text(format!(
                        "Compression level set to: {:?}",
                        level
                    ))],
                    is_error: None,
                })
            }

            _ => Err(format!("Unknown tool: {}", call.name)),
        }
    }

    pub fn get_resource(&self, uri: &str) -> Result<String, String> {
        match uri {
            "cartographer://project-graph" => {
                let graph = self.api_state.rebuild_graph()?;
                Ok(serde_json::to_string_pretty(&graph).unwrap_or_default())
            }
            "cartographer://module-index" => {
                let files = self
                    .api_state
                    .mapped_files
                    .lock()
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::to_string_pretty(&*files).unwrap_or_default())
            }
            _ => Err(format!("Unknown resource: {}", uri)),
        }
    }

    pub fn get_prompt(
        &self,
        name: &str,
        arguments: &HashMap<String, String>,
    ) -> Result<String, String> {
        match name {
            "analyze_module" => {
                let module_id = arguments
                    .get("module_id")
                    .ok_or("Missing module_id argument")?;

                let request = ModuleContextRequest {
                    module_id: module_id.clone(),
                    depth: Some(1),
                    detail_level: Some("standard".to_string()),
                    include: None,
                    format: None,
                };

                let context = self.api_state.get_module_context(&request)?;

                Ok(format!(
                    "Analyze the module at {}:\n\n\
                    Path: {}\n\n\
                    Imports:\n{}\n\n\
                    Signatures:\n{}\n\n\
                    Provide a summary of the module's public API and its dependencies.",
                    module_id,
                    context.path,
                    context.imports.join("\n"),
                    context.signatures.join("\n")
                ))
            }

            "plan_refactoring" => {
                let module_id = arguments
                    .get("module_id")
                    .ok_or("Missing module_id argument")?;
                let goal = arguments.get("goal").ok_or("Missing goal argument")?;

                let request = ModuleContextRequest {
                    module_id: module_id.clone(),
                    depth: Some(2),
                    detail_level: Some("extended".to_string()),
                    include: None,
                    format: None,
                };

                let context = self.api_state.get_module_context(&request)?;

                Ok(format!(
                    "Plan a refactoring of {} to achieve: {}\n\n\
                    Current module path: {}\n\n\
                    Dependencies (depth 2):\n{}\n\n\
                    Public API:\n{}\n\n\
                    Consider:\n\
                    1. How the refactoring affects each dependency\n\
                    2. Potential breaking changes\n\
                    3. Migration strategy",
                    module_id,
                    goal,
                    context.path,
                    context
                        .dependencies
                        .as_ref()
                        .map(|d| d
                            .iter()
                            .map(|d| d.module_id.clone())
                            .collect::<Vec<_>>()
                            .join(", "))
                        .unwrap_or_default(),
                    context.signatures.join("\n")
                ))
            }

            _ => Err(format!("Unknown prompt: {}", name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_info() {
        let info = McpServerInfo::default();
        assert_eq!(info.name, "Project Cartographer MCP Server");
        assert_eq!(info.version, "1.0.0");
    }

    #[test]
    fn test_tools_created() {
        let api_state = std::sync::Arc::new(ApiState::new(std::path::PathBuf::from("/test")));
        let server = McpServer::new(api_state);
        assert!(!server.list_tools().is_empty());
    }
}
