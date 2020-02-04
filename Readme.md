> This project is being realized in the context of my master thesis in the
> **Technische Hochschule Ulm** (Deutschland), based on an idea I had a few years back.

# Balthazar: A peer-to-peer world computer.

## Balthacli

The command-line tool to run the node and interact with the network.
It serves as an interface to **Balthalib** for the command-line and contains only code for this purpose.

## Balthalib

Contains and regroups all the base elements to use the network found in the libraries given here underneath.

## Balthastore

This part manages the different storages used by the system.
For now, only **IPFS** will be implemented but others might come in the future.

## Balthaproto

The different protocols used specifically for this system for inter-node communications.

## Balthamisc

Contains different miscelaneous types and functions used by other libraries.