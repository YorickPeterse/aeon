# frozen_string_literal: true

module Inkoc
  module VisitorMethods
    def process_node(node, *args)
      callback = node.visitor_method

      public_send(callback, node, *args) if respond_to?(callback)
    end

    def process_nodes(nodes, *args)
      nodes.map { |node| process_node(node, *args) }
    end

    def update_node(node, *args)
      callback = node.visitor_method

      if respond_to?(callback)
        public_send(callback, node, *args)
      else
        node
      end
    end

    def update_nodes(nodes, *args)
      nodes.map! { |node| update_node(node, *args) }
    end
  end
end
