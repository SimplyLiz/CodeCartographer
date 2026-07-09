#!/bin/bash
# CMP + UltraContext Demo Workflow
# This script demonstrates the complete UC integration

set -e

echo "=========================================="
echo "CMP + UltraContext Integration Demo"
echo "=========================================="
echo ""

# Check if UC API key is set
if [ -z "$ULTRA_CONTEXT" ] && [ ! -f ".env.local" ]; then
    echo "❌ UC API key not found!"
    echo "Set ULTRA_CONTEXT env var or create .env.local"
    exit 1
fi

echo "✓ UC API key found"
echo ""

# Step 1: Initialize UC sync
echo "Step 1: Initialize UC sync"
echo "-------------------------------------------"
codecartographer init --cloud --project demo-project
echo ""

# Step 2: Scan codebase
echo "Step 2: Scan codebase"
echo "-------------------------------------------"
codecartographer source
echo ""

# Step 3: Push to UC
echo "Step 3: Push to UC"
echo "-------------------------------------------"
codecartographer push
echo ""

# Step 4: View history
echo "Step 4: View version history"
echo "-------------------------------------------"
codecartographer history
echo ""

# Step 5: Create a branch
echo "Step 5: Create feature branch"
echo "-------------------------------------------"
codecartographer branch feature-demo
echo ""

# Step 6: Add some agents
echo "Step 6: Configure AI agents"
echo "-------------------------------------------"
codecartographer agents add cursor --type cursor
codecartographer agents add claude --type claude
echo ""

# Step 7: List agents
echo "Step 7: List configured agents"
echo "-------------------------------------------"
codecartographer agents list
echo ""

# Step 8: View analytics
echo "Step 8: View analytics dashboard"
echo "-------------------------------------------"
codecartographer analytics
echo ""

# Step 9: Get optimization suggestions
echo "Step 9: Get optimization suggestions"
echo "-------------------------------------------"
codecartographer optimize
echo ""

echo "=========================================="
echo "Demo Complete!"
echo "=========================================="
echo ""
echo "Your context is now:"
echo "  ✓ Scanned and cached locally"
echo "  ✓ Synced to UltraContext cloud"
echo "  ✓ Versioned with full history"
echo "  ✓ Accessible by configured agents"
echo "  ✓ Tracked with analytics"
echo ""
echo "Next steps:"
echo "  - Run 'codecartographer pull' on another machine"
echo "  - Run 'codecartographer watch' for live updates"
echo "  - Run 'codecartographer diff 0 1' to see changes"
echo ""
