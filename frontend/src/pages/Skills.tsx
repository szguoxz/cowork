import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Puzzle, Trash2, RefreshCw, Download, FolderOpen, Globe2, Info } from 'lucide-react'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../components/ui/card'
import { Button } from '../components/ui/button'
import { Input } from '../components/ui/input'
import { Badge } from '../components/ui/badge'
import { Select } from '../components/ui/select'

interface InstalledSkill {
  name: string
  description: string
  location: 'global' | 'project'
  path: string
}

export default function SkillsPage() {
  const [skills, setSkills] = useState<InstalledSkill[]>([])
  const [loading, setLoading] = useState(true)
  const [showInstallForm, setShowInstallForm] = useState(false)
  const [installUrl, setInstallUrl] = useState('')
  const [installLocation, setInstallLocation] = useState<'global' | 'project'>('project')
  const [actionLoading, setActionLoading] = useState<string | null>(null)
  const [selectedSkill, setSelectedSkill] = useState<InstalledSkill | null>(null)

  const loadSkills = async () => {
    try {
      const result = await invoke<InstalledSkill[]>('list_installed_skills')
      setSkills(result)
    } catch (err) {
      console.error('Failed to load skills:', err)
    }
  }

  const refresh = async () => {
    setLoading(true)
    await loadSkills()
    setLoading(false)
  }

  useEffect(() => {
    refresh()
  }, [])

  const installSkill = async () => {
    if (!installUrl) return
    setActionLoading('install')
    try {
      await invoke('install_skill', {
        url: installUrl,
        location: installLocation,
        force: false,
      })
      setInstallUrl('')
      setShowInstallForm(false)
      await refresh()
    } catch (err) {
      console.error('Failed to install skill:', err)
      alert(`Failed to install skill: ${err}`)
    } finally {
      setActionLoading(null)
    }
  }

  const removeSkill = async (name: string, location: string) => {
    if (!confirm(`Remove skill "${name}"?`)) return
    setActionLoading(name)
    try {
      await invoke('remove_skill', { name, location })
      await refresh()
    } catch (err) {
      console.error('Failed to remove skill:', err)
    } finally {
      setActionLoading(null)
    }
  }

  const globalSkills = skills.filter(s => s.location === 'global')
  const projectSkills = skills.filter(s => s.location === 'project')

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
          <Puzzle className="w-5 h-5 text-primary" />
          <h1 className="text-lg font-semibold">Skills</h1>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={refresh}>
            <RefreshCw className="w-4 h-4" />
            Refresh
          </Button>
          <Button size="sm" onClick={() => setShowInstallForm(true)}>
            <Download className="w-4 h-4" />
            Install Skill
          </Button>
        </div>
      </header>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-4xl mx-auto space-y-6">
          {/* Install Form */}
          {showInstallForm && (
            <Card className="animate-in border-primary/50">
              <CardHeader>
                <CardTitle className="text-lg">Install Skill from URL</CardTitle>
                <CardDescription>
                  Download and install a skill package (zip file) from a URL
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <div>
                  <label className="text-sm font-medium mb-1.5 block">Skill Package URL</label>
                  <Input
                    placeholder="https://example.com/my-skill.zip"
                    value={installUrl}
                    onChange={(e) => setInstallUrl(e.target.value)}
                  />
                </div>
                <div>
                  <label className="text-sm font-medium mb-1.5 block">Install Location</label>
                  <Select
                    value={installLocation}
                    onChange={(e) => setInstallLocation(e.target.value as 'global' | 'project')}
                  >
                    <option value="project">Project (.cowork/skills/) - Only for this project</option>
                    <option value="global">Global (~/.claude/skills/) - Available everywhere</option>
                  </Select>
                </div>
                <div className="flex gap-2 justify-end">
                  <Button variant="outline" onClick={() => setShowInstallForm(false)}>
                    Cancel
                  </Button>
                  <Button onClick={installSkill} disabled={actionLoading === 'install' || !installUrl}>
                    {actionLoading === 'install' ? (
                      <RefreshCw className="w-4 h-4 animate-spin" />
                    ) : (
                      <Download className="w-4 h-4" />
                    )}
                    Install
                  </Button>
                </div>
              </CardContent>
            </Card>
          )}

          {/* Info Card */}
          <Card className="bg-muted/50 border-muted">
            <CardContent className="p-4 flex items-start gap-3">
              <Info className="w-5 h-5 text-muted-foreground shrink-0 mt-0.5" />
              <div className="text-sm text-muted-foreground">
                <p className="font-medium text-foreground mb-1">About Skills</p>
                <p>
                  Skills are custom slash commands that extend Cowork's capabilities.
                  Each skill is a directory containing a <code className="bg-background px-1 rounded">SKILL.md</code> file
                  with instructions for the AI.
                </p>
              </div>
            </CardContent>
          </Card>

          {/* Project Skills */}
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <FolderOpen className="w-4 h-4 text-muted-foreground" />
              <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wide">
                Project Skills ({projectSkills.length})
              </h2>
            </div>

            {projectSkills.length === 0 ? (
              <Card className="border-dashed">
                <CardContent className="py-8 text-center">
                  <p className="text-sm text-muted-foreground">
                    No project-specific skills installed
                  </p>
                </CardContent>
              </Card>
            ) : (
              <div className="grid gap-3">
                {projectSkills.map((skill) => (
                  <SkillCard
                    key={skill.name}
                    skill={skill}
                    onRemove={() => removeSkill(skill.name, skill.location)}
                    onSelect={() => setSelectedSkill(skill)}
                    loading={actionLoading === skill.name}
                  />
                ))}
              </div>
            )}
          </div>

          {/* Global Skills */}
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <Globe2 className="w-4 h-4 text-muted-foreground" />
              <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wide">
                Global Skills ({globalSkills.length})
              </h2>
            </div>

            {globalSkills.length === 0 ? (
              <Card className="border-dashed">
                <CardContent className="py-8 text-center">
                  <p className="text-sm text-muted-foreground">
                    No global skills installed
                  </p>
                </CardContent>
              </Card>
            ) : (
              <div className="grid gap-3">
                {globalSkills.map((skill) => (
                  <SkillCard
                    key={skill.name}
                    skill={skill}
                    onRemove={() => removeSkill(skill.name, skill.location)}
                    onSelect={() => setSelectedSkill(skill)}
                    loading={actionLoading === skill.name}
                  />
                ))}
              </div>
            )}
          </div>

          {/* Empty State */}
          {skills.length === 0 && (
            <Card className="border-dashed">
              <CardContent className="flex flex-col items-center justify-center py-12 text-center">
                <Puzzle className="w-12 h-12 text-muted-foreground/50 mb-4" />
                <h3 className="font-medium mb-1">No custom skills installed</h3>
                <p className="text-sm text-muted-foreground mb-4">
                  Install skills to add custom slash commands to Cowork
                </p>
                <Button onClick={() => setShowInstallForm(true)}>
                  <Download className="w-4 h-4" />
                  Install Your First Skill
                </Button>
              </CardContent>
            </Card>
          )}
        </div>
      </div>

      {/* Skill Detail Modal */}
      {selectedSkill && (
        <div
          className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
          onClick={() => setSelectedSkill(null)}
        >
          <Card className="w-full max-w-lg m-4 animate-in" onClick={(e) => e.stopPropagation()}>
            <CardHeader>
              <div className="flex items-center justify-between">
                <CardTitle>/{selectedSkill.name}</CardTitle>
                <Badge variant={selectedSkill.location === 'global' ? 'secondary' : 'outline'}>
                  {selectedSkill.location}
                </Badge>
              </div>
              <CardDescription>{selectedSkill.description}</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div>
                <label className="text-sm font-medium text-muted-foreground">Path</label>
                <p className="text-sm font-mono bg-muted p-2 rounded mt-1 break-all">
                  {selectedSkill.path}
                </p>
              </div>
              <div className="flex justify-end gap-2">
                <Button variant="outline" onClick={() => setSelectedSkill(null)}>
                  Close
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  )
}

function SkillCard({
  skill,
  onRemove,
  onSelect,
  loading,
}: {
  skill: InstalledSkill
  onRemove: () => void
  onSelect: () => void
  loading: boolean
}) {
  return (
    <Card className="animate-in hover:border-primary/50 transition-colors cursor-pointer" onClick={onSelect}>
      <CardContent className="p-4">
        <div className="flex items-start justify-between">
          <div className="flex items-start gap-3">
            <div className="w-10 h-10 rounded-lg bg-primary/10 flex items-center justify-center">
              <Puzzle className="w-5 h-5 text-primary" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h3 className="font-medium font-mono">/{skill.name}</h3>
                <Badge variant={skill.location === 'global' ? 'secondary' : 'outline'} className="text-xs">
                  {skill.location}
                </Badge>
              </div>
              <p className="text-sm text-muted-foreground mt-0.5">
                {skill.description || 'No description'}
              </p>
            </div>
          </div>
          <Button
            variant="ghost"
            size="icon"
            onClick={(e) => {
              e.stopPropagation()
              onRemove()
            }}
            disabled={loading}
            title="Remove skill"
            className="text-destructive hover:text-destructive"
          >
            {loading ? (
              <RefreshCw className="w-4 h-4 animate-spin" />
            ) : (
              <Trash2 className="w-4 h-4" />
            )}
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}
