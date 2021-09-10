#!/usr/bin/env bash

function node_list()
{
	local resv_numb=$1
	preserve -long-list \
		| grep $resv_numb \
		| cut -f 9-
}

function wait_for_allocation()
{
	local resv_numb=$1
	printf "waiting for nodes " >&2
	while [ "$(node_list $resv_numb)" == "-" ]
	do
		sleep 0.25
		printf "." >&2
	done
	echo "" >&2
}

function cmd()
{
	local node=$1
	local base_cmd="$2"
	echo "ssh $node \\\"$base_cmd\\\"; sleep 90"
}

function run_in_tmux_splits()
{
	local base_cmd="$1"
	local nodes="${@:2}"
	local tmux_cmd="tmux new -s deployed"
	for node in $nodes; do
		local cmd=\"$(cmd $node "$base_cmd")\"
		tmux_cmd="$tmux_cmd "$cmd" ';' split"
	done

	eval ${tmux_cmd::-9} # print with last split removed
}

function run_in_tmux_windows()
{
	local base_cmd="$1"
	local nodes=(${@:2})
	local cmd="ssh ${nodes[0]} \"/tmp/mock-fs/discovery-exchange-id 3\"; sleep 90"
	tmux new-session -s "deployed" -n ${nodes[0]} -d "$cmd"
	local len=${#nodes[@]}
	for (( i = 1; i < $len; i++ )); do
		local name=${nodes[$i]}
		local cmd="ssh ${nodes[$i]} \"/tmp/mock-fs/discovery-exchange-id 3\"; sleep 90"
		tmux new-window -t "deployed:$i" -n $name -d "$cmd"
	done
	tmux attach-session -t "deployed"
}

function deploy()
{
	local numb_nodes=$1
	local bin=$2
	local args="${@:3}"

	local duration=0
	local resv_numb=$(preserve -# ${numb_nodes} -t 00:${duration}:05 | head -n 1 | cut -d ' ' -f 3)
	local resv_numb=${resv_numb::-1}

	node_list $resv_numb
	wait_for_allocation $resv_numb

	local nodes=$(node_list $resv_numb)
	echo "got nodes: $nodes"

	for node in $nodes; do
		ssh -t $node <<- EOF # TODO run in parallel
		mkdir -p /tmp/mock-fs
		cp ${PWD}/bin/$bin /tmp/mock-fs/
EOF
	done

	# run_in_tmux_splits "/tmp/mock-fs/$bin $args" $nodes
	run_in_tmux_windows "/tmp/mock-fs/$bin $args" $nodes
	tmux kill-session -t "deployed" 
}