// API Service - Exposes Project Cartographer via HTTP API
// This provides endpoints for AI tools like ShellAI to query module context

use crate::mapper::{DetailLevel, MappedFile};
use petgraph::algo;
use petgraph::graphmap::UnGraphMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

/// API Configuration
#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub host: String,
    pub port: u16,
    pub enable_cors: bool,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            enable_cors: true,
        }
    }
}

/// Module context request
#[derive(Debug, Deserialize)]
pub struct ModuleContextRequest {
    pub module_id: String,
    pub depth: Option<u32>,
    pub detail_level: Option<String>,
    pub include: Option<Vec<String>>,
    pub format: Option<String>,
}

/// Module context response
#[derive(Debug, Serialize)]
pub struct ModuleContextResponse {
    pub module_id: String,
    pub path: String,
    pub imports: Vec<String>,
    pub signatures: Vec<String>,
    pub docstrings: Option<Vec<String>>,
    pub parameters: Option<Vec<String>>,
    pub return_types: Option<Vec<String>>,
    pub dependencies: Option<Vec<DependencyInfo>>,
    pub detail_level: String,
}

#[derive(Debug, Serialize)]
pub struct DependencyInfo {
    pub module_id: String,
    pub path: String,
    pub signature_count: usize,
}

/// Graph query request
#[derive(Debug, Deserialize)]
pub struct GraphQueryRequest {
    pub module_id: Option<String>,
    pub query: Option<String>,
    pub query_type: Option<String>,
}

/// Project graph response
#[derive(Debug, Clone, Serialize)]
pub struct ProjectGraphResponse {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub metadata: GraphMetadata,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub module_id: String,
    pub path: String,
    pub language: String,
    pub signature_count: usize,
    pub complexity: Option<u32>,
    pub is_bridge: Option<bool>,
    pub bridge_score: Option<f64>,
    pub degree: Option<usize>,
    pub risk_level: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub edge_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphMetadata {
    pub total_files: usize,
    pub total_edges: usize,
    pub languages: HashMap<String, usize>,
    pub generated_at: String,
    pub bridge_count: Option<usize>,
}

/// Compression level configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionLevel {
    Minimal,
    Standard,
    Aggressive,
}

impl Default for CompressionLevel {
    fn default() -> Self {
        Self::Standard
    }
}

/// API State shared across requests
pub struct ApiState {
    pub root_path: std::path::PathBuf,
    pub mapped_files: Mutex<HashMap<String, MappedFile>>,
    pub project_graph: Mutex<Option<ProjectGraphResponse>>,
    pub compression_level: Mutex<CompressionLevel>,
}

impl ApiState {
    pub fn new(root_path: std::path::PathBuf) -> Self {
        Self {
            root_path,
            mapped_files: Mutex::new(HashMap::new()),
            project_graph: Mutex::new(None),
            compression_level: Mutex::new(CompressionLevel::Standard),
        }
    }

    pub fn get_module_context(
        &self,
        request: &ModuleContextRequest,
    ) -> Result<ModuleContextResponse, String> {
        let files = self.mapped_files.lock().map_err(|e| e.to_string())?;

        let module = files
            .get(&request.module_id)
            .ok_or_else(|| format!("Module not found: {}", request.module_id))?;

        let detail = match request.detail_level.as_deref() {
            Some("minimal") => DetailLevel::Minimal,
            Some("extended") => DetailLevel::Extended,
            _ => DetailLevel::Standard,
        };

        let response = ModuleContextResponse {
            module_id: request.module_id.clone(),
            path: module.path.clone(),
            imports: module.imports.clone(),
            signatures: module.signatures.clone(),
            docstrings: match detail {
                DetailLevel::Minimal => None,
                _ => module.docstrings.clone(),
            },
            parameters: match detail {
                DetailLevel::Minimal => None,
                _ => module.parameters.clone(),
            },
            return_types: match detail {
                DetailLevel::Minimal => None,
                DetailLevel::Standard => None,
                DetailLevel::Extended => module.return_types.clone(),
            },
            dependencies: self
                .get_dependencies_internal(&request.module_id, request.depth.unwrap_or(0))?,
            detail_level: format!("{:?}", detail),
        };

        Ok(response)
    }

    pub(crate) fn get_dependencies_internal(
        &self,
        module_id: &str,
        depth: u32,
    ) -> Result<Option<Vec<DependencyInfo>>, String> {
        if depth == 0 {
            return Ok(None);
        }

        let files = self.mapped_files.lock().map_err(|e| e.to_string())?;
        let graph = self.project_graph.lock().map_err(|e| e.to_string())?;

        let graph = match &*graph {
            Some(g) => g,
            None => return Ok(None),
        };

        let mut deps = Vec::new();
        let mut visited = std::collections::HashSet::new();
        visited.insert(module_id.to_string());

        self.collect_dependencies(graph, module_id, depth, &mut visited, &mut deps);

        Ok(Some(deps))
    }

    fn collect_dependencies(
        &self,
        graph: &ProjectGraphResponse,
        module_id: &str,
        remaining_depth: u32,
        visited: &mut std::collections::HashSet<String>,
        deps: &mut Vec<DependencyInfo>,
    ) {
        if remaining_depth == 0 {
            return;
        }

        for edge in &graph.edges {
            if edge.source == module_id && !visited.contains(&edge.target) {
                visited.insert(edge.target.clone());

                if let Some(node) = graph.nodes.iter().find(|n| n.module_id == edge.target) {
                    deps.push(DependencyInfo {
                        module_id: node.module_id.clone(),
                        path: node.path.clone(),
                        signature_count: node.signature_count,
                    });
                }

                self.collect_dependencies(graph, &edge.target, remaining_depth - 1, visited, deps);
            }
        }
    }

    pub fn get_dependencies(&self, module_id: &str) -> Result<Vec<DependencyInfo>, String> {
        self.get_dependencies_internal(module_id, 1)?
            .ok_or_else(|| "No dependencies found".to_string())
    }

    pub fn get_dependents(&self, module_id: &str) -> Result<Vec<DependencyInfo>, String> {
        let graph = self.project_graph.lock().map_err(|e| e.to_string())?;
        let graph = match &*graph {
            Some(g) => g,
            None => return Err("Project graph not initialized".to_string()),
        };

        let mut dependents = Vec::new();
        for edge in &graph.edges {
            if edge.target == module_id {
                if let Some(node) = graph.nodes.iter().find(|n| n.module_id == edge.source) {
                    dependents.push(DependencyInfo {
                        module_id: node.module_id.clone(),
                        path: node.path.clone(),
                        signature_count: node.signature_count,
                    });
                }
            }
        }

        Ok(dependents)
    }

    pub fn search_graph(
        &self,
        query: &str,
        query_type: Option<&str>,
    ) -> Result<Vec<GraphNode>, String> {
        let graph = self.project_graph.lock().map_err(|e| e.to_string())?;
        let graph = match &*graph {
            Some(g) => g,
            None => return Err("Project graph not initialized".to_string()),
        };

        let query_lower = query.to_lowercase();
        let nodes: Vec<GraphNode> = graph
            .nodes
            .iter()
            .filter(|n| {
                n.module_id.to_lowercase().contains(&query_lower)
                    || n.path.to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect();

        match query_type {
            Some("edge") => {
                let edges: Vec<GraphEdge> = graph
                    .edges
                    .iter()
                    .filter(|e| {
                        e.source.to_lowercase().contains(&query_lower)
                            || e.target.to_lowercase().contains(&query_lower)
                    })
                    .cloned()
                    .collect();

                let edge_node_ids: std::collections::HashSet<&String> = edges
                    .iter()
                    .flat_map(|e| vec![&e.source, &e.target])
                    .collect();

                Ok(nodes
                    .into_iter()
                    .filter(|n| edge_node_ids.contains(&n.module_id))
                    .collect())
            }
            _ => Ok(nodes),
        }
    }

    pub fn rebuild_graph(&self) -> Result<ProjectGraphResponse, String> {
        let files = self.mapped_files.lock().map_err(|e| e.to_string())?;

        let mut nodes: Vec<GraphNode> = Vec::new();
        let mut edges: Vec<GraphEdge> = Vec::new();
        let mut languages: HashMap<String, usize> = HashMap::new();

        for (module_id, file) in files.iter() {
            let language = Path::new(&file.path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("unknown")
                .to_string();

            *languages.entry(language.clone()).or_insert(0) += 1;

            nodes.push(GraphNode {
                module_id: module_id.clone(),
                path: file.path.clone(),
                language,
                signature_count: file.signatures.len(),
                complexity: None,
                is_bridge: None,
                bridge_score: None,
                degree: None,
                risk_level: None,
            });

            for import in &file.imports {
                if let Some(target) = self.resolve_import_target(import, module_id) {
                    edges.push(GraphEdge {
                        source: module_id.clone(),
                        target,
                        edge_type: "import".to_string(),
                    });
                }
            }
        }

        let bridge_analysis = self.analyze_bridges(&nodes, &edges);

        for node in &mut nodes {
            if let Some(analysis) = bridge_analysis.get(&node.module_id) {
                node.is_bridge = Some(analysis.is_bridge);
                node.bridge_score = Some(analysis.bridge_score);
                node.degree = Some(analysis.degree);
                node.risk_level = Some(analysis.risk_level.clone());
            }
        }

        let bridge_count = nodes.iter().filter(|n| n.is_bridge == Some(true)).count();

        let metadata = GraphMetadata {
            total_files: nodes.len(),
            total_edges: edges.len(),
            languages,
            generated_at: chrono_now(),
            bridge_count: Some(bridge_count),
        };

        let response = ProjectGraphResponse {
            nodes,
            edges,
            metadata,
        };

        let mut graph = self.project_graph.lock().map_err(|e| e.to_string())?;
        *graph = Some(response.clone());

        Ok(response)
    }
}

struct BridgeAnalysis {
    is_bridge: bool,
    bridge_score: f64,
    degree: usize,
    risk_level: String,
}

impl ApiState {
    fn analyze_bridges(
        &self,
        nodes: &[GraphNode],
        edges: &[GraphEdge],
    ) -> HashMap<String, BridgeAnalysis> {
        let mut graph: UnGraphMap<&str, ()> = UnGraphMap::new();

        let node_ids: HashMap<&str, &GraphNode> =
            nodes.iter().map(|n| (n.module_id.as_str(), n)).collect();

        for node in nodes {
            graph.add_node(node.module_id.as_str());
        }

        for edge in edges {
            graph.add_edge(edge.source.as_str(), edge.target.as_str(), ());
        }

        let node_count = graph.nodes().count();
        if node_count < 3 {
            return HashMap::new();
        }

        let avg_degree = 2.0 * edges.len() as f64 / node_count as f64;
        let hub_threshold = (avg_degree * 3.0).max(20.0) as usize;

        let betweenness = self.compute_betweenness_centrality(&graph);

        let mut analysis: HashMap<String, BridgeAnalysis> = HashMap::new();

        for (node_id, bc) in &betweenness {
            let degree = graph.edges(node_id).count();
            let is_hub = degree > hub_threshold;

            let normalized_bc = bc / ((node_count - 1) as f64 * (node_count - 2) as f64);
            let bridge_score = if is_hub { 0.0 } else { normalized_bc * 1000.0 };

            let is_bridge = !is_hub && bridge_score > 0.0;

            let risk_level = if is_bridge && bridge_score > 10.0 {
                "CRITICAL".to_string()
            } else if is_bridge {
                "HIGH".to_string()
            } else if is_hub {
                "LOW".to_string()
            } else {
                "MEDIUM".to_string()
            };

            analysis.insert(
                node_id.to_string(),
                BridgeAnalysis {
                    is_bridge,
                    bridge_score,
                    degree,
                    risk_level,
                },
            );
        }

        analysis
    }

    fn compute_betweenness_centrality<'a>(
        &self,
        graph: &UnGraphMap<&'a str, ()>,
    ) -> HashMap<&'a str, f64> {
        let mut betweenness = HashMap::new();
        let nodes: Vec<&str> = graph.nodes().collect();

        for node in &nodes {
            betweenness.insert(*node, 0.0);
        }

        for src in &nodes {
            let mut stack: Vec<&str> = Vec::new();
            let mut predecessors: HashMap<&str, Vec<&str>> = HashMap::new();
            let mut sigma: HashMap<&str, f64> = HashMap::new();
            let mut distance: HashMap<&str, i32> = HashMap::new();
            let mut queue: Vec<&str> = Vec::new();

            for node in &nodes {
                distance.insert(*node, -1);
                sigma.insert(*node, 0.0);
            }

            distance.insert(*src, 0);
            sigma.insert(*src, 1.0);
            queue.push(*src);

            while let Some(v) = queue.pop() {
                stack.push(v);
                let v_dist = distance.get(v).copied().unwrap_or(0);

                for w in graph.neighbors(v) {
                    if *distance.get(w).unwrap_or(&-1) == -1 {
                        distance.insert(w, v_dist + 1);
                        queue.push(w);
                    }

                    if *distance.get(w).unwrap_or(&0) == v_dist + 1 {
                        let sigma_v = sigma.get(v).copied().unwrap_or(0.0);
                        let sigma_w = sigma.get(w).copied().unwrap_or(0.0);
                        sigma.insert(w, sigma_w + sigma_v);

                        predecessors.entry(w).or_insert_with(Vec::new).push(v);
                    }
                }
            }

            let mut delta: HashMap<&str, f64> = HashMap::new();
            for node in &nodes {
                delta.insert(*node, 0.0);
            }

            while let Some(w) = stack.pop() {
                if let Some(preds) = predecessors.get(w) {
                    for v in preds {
                        let delta_v = delta.get(v).copied().unwrap_or(0.0);
                        let sigma_v = sigma.get(v).copied().unwrap_or(0.0);
                        let sigma_w = sigma.get(w).copied().unwrap_or(0.0);
                        let factor = sigma_v / sigma_w;
                        delta.insert(
                            v,
                            delta_v + factor * (1.0 + delta.get(w).copied().unwrap_or(0.0)),
                        );
                    }
                }

                if w != *src {
                    let bc_w = betweenness.get(w).copied().unwrap_or(0.0);
                    let delta_w = delta.get(w).copied().unwrap_or(0.0);
                    betweenness.insert(w, bc_w + delta_w);
                }
            }
        }

        let n = nodes.len();
        if n > 2 {
            let divisor = ((n - 1) * (n - 2)) as f64;
            for (_, bc) in betweenness.iter_mut() {
                *bc /= divisor;
            }
        }

        betweenness
    }

    fn resolve_import_target(&self, import: &str, source: &str) -> Option<String> {
        let files = self.mapped_files.lock().ok()?;

        let import_name = import
            .split_whitespace()
            .last()
            .unwrap_or(import)
            .trim_end_matches(';')
            .trim_matches('"')
            .trim_matches('\'');

        for (module_id, file) in files.iter() {
            if module_id == source {
                continue;
            }

            let file_name = Path::new(&file.path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            if import_name == file_name || import_name == &file.path {
                return Some(module_id.clone());
            }
        }

        None
    }

    pub fn set_compression_level(&self, level: CompressionLevel) {
        if let Ok(mut compression) = self.compression_level.lock() {
            *compression = level;
        }
    }

    pub fn get_compression_level(&self) -> CompressionLevel {
        self.compression_level
            .lock()
            .map(|c| *c)
            .unwrap_or(CompressionLevel::Standard)
    }
}

fn chrono_now() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_state_creation() {
        let state = ApiState::new(std::path::PathBuf::from("/test"));
        assert!(state.mapped_files.lock().unwrap().is_empty());
    }

    #[test]
    fn test_compression_level_default() {
        let level = CompressionLevel::default();
        assert_eq!(level, CompressionLevel::Standard);
    }
}
