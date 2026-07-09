# Project CodeCartographer Integration Guide

## Overview
Project CodeCartographer provides semantic workspace mapping to enhance the capabilities of AI agents like ShellAI by offering a highly compressed yet semantically rich understanding of codebases. This dramatically reduces token usage and expands the context window available to LLMs.

## Integration with ShellAI
ShellAI leverages the `get_module_context` API endpoint provided by Project CodeCartographer for targeted code queries. This API allows ShellAI to:
- **Retrieve Public API Surface**: Get the public API signatures of any specific module.
- **Include Transitive Dependencies**: Optionally include dependencies to understand how a module interacts with others.
- **Benefit from Compressed Format**: The context is delivered in a highly compressed format using AI Lang techniques, minimizing token usage for each query.

This integration empowers ShellAI to perform precise code analysis and answer questions about specific parts of the codebase with a much smaller context footprint, leading to faster and more accurate results.