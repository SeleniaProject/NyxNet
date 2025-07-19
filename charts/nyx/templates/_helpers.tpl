{{- define "nyx.name" -}}
nyx
{{- end -}}

{{- define "nyx.fullname" -}}
{{ include "nyx.name" . }}
{{- end -}} 