// API Service - Exposes Project Cartographer via HTTP API
// This provides endpoints for AI tools like ShellAI to query module context

use crate::layers::{detect_layer_violations, LayerConfig, LayerViolation};
use crate::mapper::{DetailLevel, MappedFile, Signature};
use petgraph::algo;
use petgraph::graphmap::{DiGraphMap, UnGraphMap};
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
    pub signatures: Vec<Signature>,
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
    pub cycles: Vec<CycleInfo>,
    pub god_modules: Vec<GodModuleInfo>,
    pub layer_violations: Vec<LayerViolation>,
    pub metadata: GraphMetadata,
    /// Temporal coupling pairs from git history (populated by enrich_with_git).
    pub cochange_pairs: Vec<CoChangePair>,
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
    /// Number of commits that touched this file (from git history).
    pub churn: Option<usize>,
    /// churn × signature_count, normalised 0–100.
    pub hotspot_score: Option<f64>,
    /// Architectural role: entry/core/utility/leaf/dead/bridge/standard.
    pub role: Option<String>,
    /// True when no other module imports this file and it is not an entry point.
    pub is_dead: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub edge_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoChangePair {
    pub file_a: String,
    pub file_b: String,
    pub count: usize,
    pub coupling_score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphMetadata {
    pub total_files: usize,
    pub total_edges: usize,
    pub languages: HashMap<String, usize>,
    pub generated_at: String,
    pub bridge_count: Option<usize>,
    pub cycle_count: Option<usize>,
    pub god_module_count: Option<usize>,
    pub health_score: Option<f64>,
    pub layer_violation_count: Option<usize>,
    pub architectural_drift: Option<f64>,
    pub hotspot_count: Option<usize>,
    pub dead_code_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedChange {
    pub target_module: String,
    pub new_signature: Option<String>,
    pub removed_signature: Option<String>,
    pub predicted_impact: ImpactAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAnalysis {
    pub affected_modules: Vec<String>,
    pub callers_count: usize,
    pub callees_count: usize,
    pub will_create_cycle: bool,
    pub layer_violations: Vec<LayerViolation>,
    pub risk_level: String,
    pub health_impact: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureSnapshot {
    pub timestamp: u64,
    pub health_score: f64,
    pub total_files: usize,
    pub total_edges: usize,
    pub bridge_count: usize,
    pub cycle_count: usize,
    pub god_module_count: usize,
    pub layer_violation_count: usize,
    pub dominant_language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureEvolution {
    pub snapshots: Vec<ArchitectureSnapshot>,
    pub health_trend: String,
    pub debt_indicators: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CycleInfo {
    pub nodes: Vec<String>,
    pub pivot_node: Option<String>,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GodModuleInfo {
    pub module_id: String,
    pub path: String,
    pub degree: usize,
    pub cohesion_score: f64,
    pub severity: String,
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
                churn: None,
                hotspot_score: None,
                role: None,
                is_dead: None,
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

        let cycles = self.detect_cycles(&nodes, &edges);
        let cycle_count = cycles.len();

        let god_modules = self.detect_god_modules(&nodes, &edges, &files);
        let god_module_count = god_modules.len();

        let edge_tuples: Vec<(String, String)> = edges
            .iter()
            .map(|e| (e.source.clone(), e.target.clone()))
            .collect();

        let layer_violations = self.detect_layer_violations(&edge_tuples);
        let layer_violation_count = layer_violations.len();

        let health_score = self.calculate_health_score(
            bridge_count,
            cycle_count,
            god_module_count,
            layer_violation_count,
            nodes.len(),
        );

        // --- Role classification and dead-code detection ---
        // Compute per-node in/out degree from the edge list.
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut out_degree: HashMap<String, usize> = HashMap::new();
        for node in &nodes {
            in_degree.entry(node.module_id.clone()).or_insert(0);
            out_degree.entry(node.module_id.clone()).or_insert(0);
        }
        for edge in &edges {
            *out_degree.entry(edge.source.clone()).or_insert(0) += 1;
            *in_degree.entry(edge.target.clone()).or_insert(0) += 1;
        }

        let mut dead_code_count = 0usize;

        for node in &mut nodes {
            let ind = *in_degree.get(&node.module_id).unwrap_or(&0);
            let outd = *out_degree.get(&node.module_id).unwrap_or(&0);

            let is_entry_name = is_entry_point_path(&node.path);
            let is_test = is_test_path(&node.path);

            // Role assignment (bridge takes priority over other roles).
            node.role = Some(if node.is_bridge == Some(true) {
                "bridge".to_string()
            } else if ind == 0 && outd == 0 && !is_entry_name && !is_test {
                "dead".to_string()
            } else if ind == 0 && outd > 0 && !is_test {
                "entry".to_string()
            } else if ind >= 5 && outd >= 3 {
                "core".to_string()
            } else if ind >= 5 {
                "utility".to_string()
            } else if outd == 0 && ind > 0 {
                "leaf".to_string()
            } else {
                "standard".to_string()
            });

            // Dead-code flag: in_degree == 0 AND not an entry point or test.
            let dead = ind == 0 && !is_entry_name && !is_test;
            node.is_dead = Some(dead);
            if dead {
                dead_code_count += 1;
            }
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string();

        let metadata = GraphMetadata {
            total_files: nodes.len(),
            total_edges: edges.len(),
            languages,
            generated_at: now,
            bridge_count: Some(bridge_count),
            cycle_count: Some(cycle_count),
            god_module_count: Some(god_module_count),
            health_score: Some(health_score),
            layer_violation_count: Some(layer_violation_count),
            architectural_drift: None,
            hotspot_count: None, // filled by enrich_with_git
            dead_code_count: Some(dead_code_count),
        };

        let response = ProjectGraphResponse {
            nodes,
            edges,
            cycles,
            god_modules,
            layer_violations,
            metadata,
            cochange_pairs: vec![],
        };

        let mut graph = self.project_graph.lock().map_err(|e| e.to_string())?;
        *graph = Some(response.clone());

        Ok(response)
    }

}

// ---------------------------------------------------------------------------
// Role-classification helpers (free functions, not methods)
// ---------------------------------------------------------------------------

pub fn is_entry_point_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(
        name,
        "main.rs"
            | "main.py"
            | "main.go"
            | "main.ts"
            | "main.js"
            | "index.ts"
            | "index.js"
            | "index.tsx"
            | "index.jsx"
            | "app.rs"
            | "app.py"
            | "app.ts"
            | "app.js"
            | "server.ts"
            | "server.js"
            | "server.go"
    )
}

fn is_test_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("_test.")
        || lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.contains("/test/")
        || lower.contains("/tests/")
        || lower.contains("/spec/")
        || lower.ends_with("_test.go")
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

            // bc is already normalized by (n-1)*(n-2) inside compute_betweenness_centrality
            let bridge_score = if is_hub { 0.0 } else { bc * 1000.0 };

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
            let mut queue: std::collections::VecDeque<&str> = std::collections::VecDeque::new();

            for node in &nodes {
                distance.insert(*node, -1);
                sigma.insert(*node, 0.0);
            }

            distance.insert(*src, 0);
            sigma.insert(*src, 1.0);
            queue.push_back(*src);

            while let Some(v) = queue.pop_front() {
                stack.push(v);
                let v_dist = distance.get(v).copied().unwrap_or(0);

                for w in graph.neighbors(v) {
                    if *distance.get(w).unwrap_or(&-1) == -1 {
                        distance.insert(w, v_dist + 1);
                        queue.push_back(w);
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

    fn detect_cycles(&self, nodes: &[GraphNode], edges: &[GraphEdge]) -> Vec<CycleInfo> {
        let mut graph: DiGraphMap<&str, ()> = DiGraphMap::new();

        for node in nodes {
            graph.add_node(node.module_id.as_str());
        }

        for edge in edges {
            graph.add_edge(edge.source.as_str(), edge.target.as_str(), ());
        }

        let sccs = petgraph::algo::tarjan_scc(&graph);

        let hub_nodes: std::collections::HashSet<&str> = nodes
            .iter()
            .filter(|n| n.degree.unwrap_or(0) > 30)
            .map(|n| n.module_id.as_str())
            .collect();

        let mut cycles = Vec::new();

        for component in sccs {
            if component.len() > 1 {
                let cycle_nodes: Vec<String> = component.iter().map(|s| s.to_string()).collect();

                let filtered_nodes: Vec<&str> = component
                    .iter()
                    .map(|&s| s)
                    .filter(|n| !hub_nodes.contains(*n))
                    .collect();

                let pivot = if filtered_nodes.is_empty() {
                    None
                } else {
                    Some(filtered_nodes[filtered_nodes.len() / 2].to_string())
                };

                let severity = if component.len() > 5 {
                    "CRITICAL"
                } else {
                    "HIGH"
                };

                cycles.push(CycleInfo {
                    nodes: cycle_nodes,
                    pivot_node: pivot,
                    severity: severity.to_string(),
                });
            }
        }

        cycles
    }

    fn detect_god_modules(
        &self,
        nodes: &[GraphNode],
        edges: &[GraphEdge],
        files: &HashMap<String, MappedFile>,
    ) -> Vec<GodModuleInfo> {
        let god_threshold = 50;
        let mut god_modules = Vec::new();

        for node in nodes {
            let degree = node.degree.unwrap_or(0);

            if degree > god_threshold {
                let file = files.get(&node.module_id);

                let import_types: std::collections::HashSet<&str> = file
                    .map(|f| {
                        f.imports
                            .iter()
                            .filter_map(|i| {
                                let parts: Vec<&str> = i.split('/').collect();
                                parts.get(1).or(parts.first()).map(|s| *s)
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let unique_types = import_types.len() as f64;
                let cohesion = if degree > 0 {
                    (unique_types / degree as f64).min(1.0)
                } else {
                    0.0
                };

                if cohesion < 0.3 {
                    let severity = if degree > 100 {
                        "CRITICAL"
                    } else if degree > 75 {
                        "HIGH"
                    } else {
                        "MEDIUM"
                    };

                    god_modules.push(GodModuleInfo {
                        module_id: node.module_id.clone(),
                        path: node.path.clone(),
                        degree,
                        cohesion_score: cohesion,
                        severity: severity.to_string(),
                    });
                }
            }
        }

        god_modules.sort_by(|a, b| b.degree.cmp(&a.degree));
        god_modules
    }

    fn calculate_health_score(
        &self,
        bridge_count: usize,
        cycle_count: usize,
        god_module_count: usize,
        layer_violation_count: usize,
        total_nodes: usize,
    ) -> f64 {
        if total_nodes == 0 {
            return 100.0;
        }

        let base_score = 100.0;
        let cycle_penalty = (cycle_count as f64 * 5.0).min(30.0);
        let bridge_penalty = ((bridge_count as f64 / total_nodes as f64) * 100.0 * 2.0).min(20.0);
        let god_penalty = (god_module_count as f64 * 3.0).min(20.0);
        let layer_penalty = (layer_violation_count as f64 * 4.0).min(25.0);

        (base_score - cycle_penalty - bridge_penalty - god_penalty - layer_penalty).max(0.0)
    }

    fn detect_layer_violations(&self, edges: &[(String, String)]) -> Vec<LayerViolation> {
        let config = LayerConfig::default();
        detect_layer_violations(edges, &config)
    }

    pub fn simulate_change(
        &self,
        module_id: &str,
        new_signature: Option<&str>,
        removed_signature: Option<&str>,
    ) -> Result<SimulatedChange, String> {
        let graph = self.rebuild_graph()?;

        let target_node = graph
            .nodes
            .iter()
            .find(|n| n.module_id == module_id)
            .ok_or_else(|| format!("Module not found: {}", module_id))?;

        let mut affected = Vec::new();
        let mut callers_count = 0;
        let mut callees_count = 0;

        for edge in &graph.edges {
            if edge.target == module_id {
                callers_count += 1;
                affected.push(edge.source.clone());
            }
            if edge.source == module_id {
                callees_count += 1;
                affected.push(edge.target.clone());
            }
        }

        let will_create_cycle = self.check_would_create_cycle(&graph.edges, module_id);

        let risk_level = if will_create_cycle {
            "CRITICAL".to_string()
        } else if callers_count > 10 {
            "HIGH".to_string()
        } else if callers_count > 5 {
            "MEDIUM".to_string()
        } else {
            "LOW".to_string()
        };

        let health_impact = if will_create_cycle {
            -15.0
        } else if callers_count > 10 {
            -5.0
        } else if callers_count > 5 {
            -2.0
        } else {
            -0.5
        };

        let mut layer_violations = Vec::new();
        if let Some(ns) = new_signature {
            for affected_module in &affected {
                let edge = (affected_module.clone(), module_id.to_string());
                let violations = detect_layer_violations(&[edge], &LayerConfig::default());
                layer_violations.extend(violations);
            }
        }

        Ok(SimulatedChange {
            target_module: module_id.to_string(),
            new_signature: new_signature.map(String::from),
            removed_signature: removed_signature.map(String::from),
            predicted_impact: ImpactAnalysis {
                affected_modules: affected,
                callers_count,
                callees_count,
                will_create_cycle,
                layer_violations,
                risk_level,
                health_impact,
            },
        })
    }

    fn check_would_create_cycle(&self, edges: &[GraphEdge], target_module: &str) -> bool {
        let mut graph: DiGraphMap<&str, ()> = DiGraphMap::new();

        for edge in edges {
            if edge.source != target_module && edge.target != target_module {
                graph.add_node(edge.source.as_str());
                graph.add_node(edge.target.as_str());
                graph.add_edge(edge.source.as_str(), edge.target.as_str(), ());
            }
        }

        graph.add_node(target_module);

        for edge in edges {
            if edge.source == target_module {
                graph.add_edge(target_module, edge.target.as_str(), ());
            }
            if edge.target == target_module {
                graph.add_edge(edge.source.as_str(), target_module, ());
            }
        }

        let sccs = petgraph::algo::tarjan_scc(&graph);
        sccs.iter()
            .any(|c| c.len() > 1 && c.contains(&target_module))
    }

    pub fn get_evolution(&self, days: Option<u32>) -> Result<ArchitectureEvolution, String> {
        let current_graph = self.rebuild_graph()?;

        let current_health = current_graph.metadata.health_score.unwrap_or(100.0);

        let days = days.unwrap_or(30);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut snapshots = vec![ArchitectureSnapshot {
            timestamp: now,
            health_score: current_health,
            total_files: current_graph.metadata.total_files,
            total_edges: current_graph.metadata.total_edges,
            bridge_count: current_graph.metadata.bridge_count.unwrap_or(0),
            cycle_count: current_graph.metadata.cycle_count.unwrap_or(0),
            god_module_count: current_graph.metadata.god_module_count.unwrap_or(0),
            layer_violation_count: current_graph.metadata.layer_violation_count.unwrap_or(0),
            dominant_language: current_graph
                .metadata
                .languages
                .iter()
                .max_by_key(|(_, v)| *v)
                .map(|(k, _)| k.clone()),
        }];

        // Trend requires multiple snapshots; this reflects current state only.
        // Historical tracking is not yet implemented, so `days` has no effect.
        let health_trend = if current_health >= 80.0 {
            "Healthy".to_string()
        } else if current_health >= 60.0 {
            "Moderate".to_string()
        } else {
            "At Risk".to_string()
        };

        let mut debt_indicators = Vec::new();
        if current_graph.metadata.cycle_count.unwrap_or(0) > 0 {
            debt_indicators.push("Active circular dependencies detected".to_string());
        }
        if current_graph.metadata.god_module_count.unwrap_or(0) > 0 {
            debt_indicators.push(format!(
                "{} god modules require attention",
                current_graph.metadata.god_module_count.unwrap_or(0)
            ));
        }
        if current_graph.metadata.layer_violation_count.unwrap_or(0) > 0 {
            debt_indicators.push(format!(
                "{} architectural boundary violations",
                current_graph.metadata.layer_violation_count.unwrap_or(0)
            ));
        }

        let mut recommendations = Vec::new();
        if current_health < 60.0 {
            recommendations.push("Critical: Immediate architectural review needed".to_string());
        }
        if current_graph.metadata.cycle_count.unwrap_or(0) > 0 {
            recommendations.push("Priority: Break circular dependencies".to_string());
        }
        if current_graph.metadata.god_module_count.unwrap_or(0) > 2 {
            recommendations
                .push("Consider splitting large modules to improve cohesion".to_string());
        }
        if recommendations.is_empty() {
            recommendations
                .push("Architecture is healthy - maintain current practices".to_string());
        }

        Ok(ArchitectureEvolution {
            snapshots,
            health_trend,
            debt_indicators,
            recommendations,
        })
    }
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
