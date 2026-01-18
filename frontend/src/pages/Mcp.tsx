import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Server, Plus, Play, Square, Trash2, RefreshCw, Wrench, Globe, Terminal } from 'lucide-react'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../components/ui/card'
import { Button } from '../components/ui/button'
import { Input } from '../components/ui/input'
import { Badge } from '../components/ui/badge'

interface McpServer {
  name: string
  command: string
  enabled: boolean
  status: 'stopped' | 'starting' | 'running' | 'failed'
  tool_count: number
  error?: string
}

interface McpTool {
  name: string
  description: string
  server: string
}

export default function McpPage() {
  const [servers, setServers] = useState<McpServer[]>([])
  const [tools, setTools] = useState<McpTool[]>([])
  const [loading, setLoading] = useState(true)
  const [showAddForm, setShowAddForm] = useState(false)
  const [newServer, setNewServer] = useState({ name: '', command: '' })
  const [actionLoading, setActionLoading] = useState<string | null>(null)

  const loadServers = async () => {
    try {
      const result = await invoke<McpServer[]>('list_mcp_servers')
      setServers(result)
    } catch (err) {
      console.error('Failed to load MCP servers:', err)
    }
  }

  const loadTools = async () => {
    try {
      const result = await invoke<McpTool[]>('list_mcp_tools')
      setTools(result)
    } catch (err) {
      console.error('Failed to load MCP tools:', err)
    }
  }

  const refresh = async () => {
    setLoading(true)
    await Promise.all([loadServers(), loadTools()])
    setLoading(false)
  }

  useEffect(() => {
    refresh()
  }, [])

  const addServer = async () => {
    if (!newServer.name || !newServer.command) return
    setActionLoading('add')
    try {
      await invoke('add_mcp_server', {
        name: newServer.name,
        command: newServer.command,
      })
      setNewServer({ name: '', command: '' })
      setShowAddForm(false)
      await refresh()
    } catch (err) {
      console.error('Failed to add server:', err)
    } finally {
      setActionLoading(null)
    }
  }

  const startServer = async (name: string) => {
    setActionLoading(name)
    try {
      await invoke('start_mcp_server', { name })
      await refresh()
    } catch (err) {
      console.error('Failed to start server:', err)
    } finally {
      setActionLoading(null)
    }
  }

  const stopServer = async (name: string) => {
    setActionLoading(name)
    try {
      await invoke('stop_mcp_server', { name })
      await refresh()
    } catch (err) {
      console.error('Failed to stop server:', err)
    } finally {
      setActionLoading(null)
    }
  }

  const removeServer = async (name: string) => {
    if (!confirm(`Remove MCP server "${name}"?`)) return
    setActionLoading(name)
    try {
      await invoke('remove_mcp_server', { name })
      await refresh()
    } catch (err) {
      console.error('Failed to remove server:', err)
    } finally {
      setActionLoading(null)
    }
  }

  const getStatusBadge = (status: string) => {
    switch (status) {
      case 'running':
        return <Badge variant="success">Running</Badge>
      case 'starting':
        return <Badge variant="secondary">Starting...</Badge>
      case 'failed':
        return <Badge variant="destructive">Failed</Badge>
      default:
        return <Badge variant="outline">Stopped</Badge>
    }
  }

  const isUrl = (cmd: string) => cmd.startsWith('http://') || cmd.startsWith('https://')

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <RefreshCw className="w-8 h-8 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <header className="h-14 border-b border-border flex items-center justify-between px-6">
        <div className="flex items-center gap-3">
          <Server className="w-5 h-5 text-primary" />
          <h1 className="text-lg font-semibold">MCP Servers</h1>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={refresh}>
            <RefreshCw className="w-4 h-4" />
            Refresh
          </Button>
          <Button size="sm" onClick={() => setShowAddForm(true)}>
            <Plus className="w-4 h-4" />
            Add Server
          </Button>
        </div>
      </header>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-4xl mx-auto space-y-6">
          {/* Add Server Form */}
          {showAddForm && (
            <Card className="animate-in border-primary/50">
              <CardHeader>
                <CardTitle className="text-lg">Add MCP Server</CardTitle>
                <CardDescription>
                  Add a stdio server (command) or HTTP server (URL)
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <label className="text-sm font-medium mb-1.5 block">Name</label>
                    <Input
                      placeholder="my-server"
                      value={newServer.name}
                      onChange={(e) => setNewServer({ ...newServer, name: e.target.value })}
                    />
                  </div>
                  <div>
                    <label className="text-sm font-medium mb-1.5 block">Command or URL</label>
                    <Input
                      placeholder="npx @modelcontextprotocol/server-filesystem or https://..."
                      value={newServer.command}
                      onChange={(e) => setNewServer({ ...newServer, command: e.target.value })}
                    />
                  </div>
                </div>
                <div className="flex gap-2 justify-end">
                  <Button variant="outline" onClick={() => setShowAddForm(false)}>
                    Cancel
                  </Button>
                  <Button onClick={addServer} disabled={actionLoading === 'add'}>
                    {actionLoading === 'add' ? (
                      <RefreshCw className="w-4 h-4 animate-spin" />
                    ) : (
                      <Plus className="w-4 h-4" />
                    )}
                    Add Server
                  </Button>
                </div>
              </CardContent>
            </Card>
          )}

          {/* Servers List */}
          <div className="space-y-3">
            <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wide">
              Configured Servers ({servers.length})
            </h2>

            {servers.length === 0 ? (
              <Card className="border-dashed">
                <CardContent className="flex flex-col items-center justify-center py-12 text-center">
                  <Server className="w-12 h-12 text-muted-foreground/50 mb-4" />
                  <h3 className="font-medium mb-1">No MCP servers configured</h3>
                  <p className="text-sm text-muted-foreground mb-4">
                    Add an MCP server to extend AI capabilities with custom tools
                  </p>
                  <Button onClick={() => setShowAddForm(true)}>
                    <Plus className="w-4 h-4" />
                    Add Your First Server
                  </Button>
                </CardContent>
              </Card>
            ) : (
              servers.map((server) => (
                <Card key={server.name} className="animate-in">
                  <CardContent className="p-4">
                    <div className="flex items-start justify-between">
                      <div className="flex items-start gap-3">
                        <div className={`
                          w-10 h-10 rounded-lg flex items-center justify-center
                          ${server.status === 'running' ? 'bg-green-500/10' : 'bg-muted'}
                        `}>
                          {isUrl(server.command) ? (
                            <Globe className={`w-5 h-5 ${server.status === 'running' ? 'text-green-500' : 'text-muted-foreground'}`} />
                          ) : (
                            <Terminal className={`w-5 h-5 ${server.status === 'running' ? 'text-green-500' : 'text-muted-foreground'}`} />
                          )}
                        </div>
                        <div>
                          <div className="flex items-center gap-2">
                            <h3 className="font-medium">{server.name}</h3>
                            {getStatusBadge(server.status)}
                            {server.tool_count > 0 && (
                              <Badge variant="secondary">
                                <Wrench className="w-3 h-3 mr-1" />
                                {server.tool_count} tools
                              </Badge>
                            )}
                          </div>
                          <p className="text-sm text-muted-foreground mt-0.5 font-mono">
                            {server.command.length > 60
                              ? server.command.substring(0, 60) + '...'
                              : server.command}
                          </p>
                          {server.error && (
                            <p className="text-sm text-destructive mt-1">{server.error}</p>
                          )}
                        </div>
                      </div>
                      <div className="flex items-center gap-1">
                        {server.status === 'running' ? (
                          <Button
                            variant="ghost"
                            size="icon"
                            onClick={() => stopServer(server.name)}
                            disabled={actionLoading === server.name}
                            title="Stop server"
                          >
                            {actionLoading === server.name ? (
                              <RefreshCw className="w-4 h-4 animate-spin" />
                            ) : (
                              <Square className="w-4 h-4" />
                            )}
                          </Button>
                        ) : (
                          <Button
                            variant="ghost"
                            size="icon"
                            onClick={() => startServer(server.name)}
                            disabled={actionLoading === server.name}
                            title="Start server"
                          >
                            {actionLoading === server.name ? (
                              <RefreshCw className="w-4 h-4 animate-spin" />
                            ) : (
                              <Play className="w-4 h-4" />
                            )}
                          </Button>
                        )}
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => removeServer(server.name)}
                          disabled={actionLoading === server.name}
                          title="Remove server"
                          className="text-destructive hover:text-destructive"
                        >
                          <Trash2 className="w-4 h-4" />
                        </Button>
                      </div>
                    </div>
                  </CardContent>
                </Card>
              ))
            )}
          </div>

          {/* Tools List */}
          {tools.length > 0 && (
            <div className="space-y-3">
              <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wide">
                Available Tools ({tools.length})
              </h2>
              <Card>
                <CardContent className="p-0">
                  <div className="divide-y divide-border">
                    {tools.map((tool, i) => (
                      <div key={i} className="p-4 flex items-start gap-3">
                        <Wrench className="w-4 h-4 text-muted-foreground mt-0.5" />
                        <div>
                          <div className="flex items-center gap-2">
                            <span className="font-medium font-mono text-sm">{tool.name}</span>
                            <Badge variant="outline" className="text-xs">{tool.server}</Badge>
                          </div>
                          <p className="text-sm text-muted-foreground mt-0.5">
                            {tool.description || 'No description'}
                          </p>
                        </div>
                      </div>
                    ))}
                  </div>
                </CardContent>
              </Card>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
