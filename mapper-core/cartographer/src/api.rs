// Add constants at the top of the file
const MAX_FILES_IN_PROJECT: usize = 100_000;
const MAX_EDGES_PER_FILE: usize = 1_000;
const MAX_TOTAL_EDGES: usize = 1_000_000;
const MAX_GRAPH_NODES: usize = 100_000;

impl ApiState {
    pub fn rebuild_graph(&self) -> Result<ProjectGraphResponse, String> {
        let files = self.mapped_files.lock().map_err(|e| e.to_string())?;

        // GUARD 1: Cap total files
        if files.len() > MAX_FILES_IN_PROJECT {
            return Err(format!(
                "Project exceeds file limit: {} > {}",
                files.len(),
                MAX_FILES_IN_PROJECT
            ));
        }

        let mut nodes: Vec<GraphNode> = Vec::with_capacity(files.len());
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
                unreferenced_exports: None,
                fan_in: None,
                fan_out: None,
                cochange_partners: None,
                cochange_entropy: None,
                owner: None,
            });

            // GUARD 2: Cap edges per file
            let import_count = file.imports.len().min(MAX_EDGES_PER_FILE);
            for import in file.imports.iter().take(import_count) {
                if let Some(target) = Self::resolve_import_target_in(&files, import, module_id) {
                    edges.push(GraphEdge {
                        source: module_id.clone(),
                        target,
                        edge_type: "import".to_string(),
                        at_range: None,
                    });
                }
            }

            // GUARD 3: Cap total edges
            if edges.len() > MAX_TOTAL_EDGES {
                return Err(format!(
                    "Project graph exceeds edge limit: {} > {}",
                    edges.len(),
                    MAX_TOTAL_EDGES
                ));
            }
        }

        // GUARD 4: Cap graph nodes (should never happen if we capped files, but defense-in-depth)
        if nodes.len() > MAX_GRAPH_NODES {
            return Err(format!(
                "Graph node count exceeds limit: {} > {}",
                nodes.len(),
                MAX_GRAPH_NODES
            ));
        }

        // ... rest of function unchanged ...
        let bridge_analysis = self.analyze_bridges(&nodes, &edges);
        // ... continue with cycle detection, health scoring, etc. ...

        Ok(response) // response struct as before
    }
}
