# frozen_string_literal: true

module Inkoc
  module Pass
    class ValidateConstraints
      include VisitorMethods

      def initialize(mod, state)
        @module = mod
        @state = state
      end

      def diagnostics
        @state.diagnostics
      end

      def run(node)
        process_nodes(node.expressions)

        [node]
      end

      def on_body(node)
        process_nodes(node.expressions)
      end

      def on_block(node)
        process_nodes(node.body.expressions)
      end
      alias on_lambda on_block

      def on_send(node)
        name = node.name
        rtype = node.receiver&.type

        if rtype&.unresolved_constraint? && !rtype.responds_to_message?(name)
          diagnostics.undefined_method_error(rtype, name, node.location)
        end

        process_node(node.receiver) if node.receiver
        process_nodes(node.arguments)
      end

      def on_raw_instruction(node)
        process_nodes(node.arguments)
      end

      def on_node_with_body(node)
        process_node(node.body)
      end

      alias on_object on_node_with_body
      alias on_trait on_node_with_body
      alias on_trait_implementation on_node_with_body
      alias on_reopen_object on_node_with_body
      alias on_method on_node_with_body

      def on_try(node)
        process_node(node.expression)
        process_node(node.else_body) if node.else_body
      end

      def on_node_with_value(node)
        process_node(node.value) if node.value
      end

      alias on_throw on_node_with_value
      alias on_return on_node_with_value
      alias on_define_variable on_node_with_value
      alias on_define_variable_with_explicit_type on_node_with_value
      alias on_reassign_variable on_node_with_value
      alias on_keyword_argument on_node_with_value
      alias on_destructure_array on_node_with_value

      def on_type_cast(node)
        process_node(node.expression)
      end
    end
  end
end
