$ErrorActionPreference = 'Stop'

$allArgs = @($args)
$subcmd = ''
for ($i = 0; $i -lt $allArgs.Count; $i++) {
    switch ($allArgs[$i]) {
        '-C' { $i++; continue }
        '--json' { continue }
        '-j' { continue }
        default {
            $subcmd = $allArgs[$i]
            break
        }
    }
    if ($subcmd) { break }
}

$joined = " $($allArgs -join ' ') "
if ($joined -like '* MISSING *') {
    [Console]::Error.WriteLine("error: knot 'MISSING' not found")
    exit 1
}

switch ($subcmd) {
    'ls' {
@'
{
  "data": [
    {
      "id": "k1",
      "title": "demo",
      "state": "ready",
      "updated_at": "t",
      "type": "work",
      "tags": []
    }
  ],
  "total": 1,
  "offset": 0,
  "limit": 50,
  "has_more": false
}
'@
    }
    'show' {
@'
{
  "id": "k1",
  "title": "demo",
  "state": "ready",
  "updated_at": "t",
  "type": "work",
  "tags": []
}
'@
    }
    'poll' {
@'
{
  "id": "k1",
  "title": "demo",
  "state": "ready_for_implementation",
  "updated_at": "t",
  "type": "work",
  "tags": []
}
'@
    }
    'new' {
@'
{
  "id": "k-new",
  "title": "New",
  "state": "ready_for_implementation",
  "updated_at": "t",
  "type": "work",
  "tags": []
}
'@
    }
    'update' {
        if ($joined -like '* k-new *') {
@'
{
  "id": "k-new",
  "title": "New",
  "state": "ready_for_implementation",
  "updated_at": "t",
  "type": "work",
  "priority": 3,
  "tags": []
}
'@
        } else {
@'
{
  "id": "k1",
  "title": "updated",
  "state": "ready_for_implementation",
  "updated_at": "t",
  "type": "work",
  "tags": []
}
'@
        }
    }
    'claim' {
        $leasePresent = ($joined -like '* --lease L1 *').ToString().ToLowerInvariant()
        if ($joined -like '* --e2e *') {
@"
{
  "id": "k1",
  "title": "demo",
  "state": "planning",
  "prompt": "do x",
  "e2e": true,
  "workflow_boundary_kind": "e2e_continuation",
  "lease_id": "L1",
  "lease_present": $leasePresent
}
"@
        } else {
@"
{
  "id": "k1",
  "title": "demo",
  "state": "planning",
  "prompt": "do x",
  "e2e": false,
  "workflow_boundary_kind": "single_action",
  "lease_id": "L1",
  "lease_present": $leasePresent
}
"@
        }
    }
    'next' {
@'
{
  "id": "k1",
  "title": "demo",
  "state": "ready_for_review",
  "updated_at": "t",
  "type": "work",
  "tags": []
}
'@
    }
    'rollback' {
        $leasePresent = ($joined -like '* --lease L1 *').ToString().ToLowerInvariant()
@"
{
  "id": "k1",
  "state": "implementation",
  "target_state": "ready_for_implementation",
  "owner_kind": "agent",
  "reason": "rolled back",
  "dry_run": false,
  "lease_present": $leasePresent
}
"@
    }
    'push' {
        if ($env:KNOTS_ALLOW_ACTIVE_LEASE_REPLICATION -eq '1') {
            '{"allow_active_leases":true}'
        } else {
            '{"allow_active_leases":false}'
        }
    }
    'sync' {
        '{"status":"deferred","active_leases":1}'
    }
    'lease' {
        if ($joined -like '* --agent-name other-client *') {
            '{"id":"L2","title":"mcp-session","state":"active","agent_info":{"model":"other"}}'
        } else {
            '{"id":"L1","title":"mcp-session","state":"active","agent_info":{"model":"test-model"}}'
        }
    }
    default {
        '{}'
    }
}
