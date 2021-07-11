# frozen_string_literal: true

module Inkoc
  module Pass
    class SetupSymbolTables
      include VisitorMethods

      def initialize(compiler, mod)
        @module = mod
        @state = compiler.state
      end

      def run(node)
        on_module_body(node)

        [node]
      end

      def on_module_body(node)
        node.locals = @module.body.locals

        process_nodes(node.expressions, node)
      end

      def on_body(node, outer)
        node.locals = SymbolTable.new

        process_nodes(node.expressions, outer)
      end

      def on_group(node, outer)
        process_nodes(node.expressions, outer)
      end

      def on_block(node, outer)
        node.body.locals = SymbolTable.new(outer.locals)

        process_nodes(node.arguments, node.body)
        process_nodes(node.body.expressions, node.body)
      end

      def on_lambda(node, _)
        node.body.locals = SymbolTable.new

        process_nodes(node.arguments, node.body)
        process_nodes(node.body.expressions, node.body)
      end

      def on_match(node, outer)
        process_node(node.expression, outer) if node.expression
        process_nodes(node.arms, outer)
        process_node(node.match_else, outer) if node.match_else
      end

      def on_match_else(node, outer)
        process_node(node.body, outer)
      end

      def on_match_type(node, outer)
        process_nodes(node.body.expressions, outer)
        process_node(node.guard, outer) if node.guard
      end

      def on_match_expression(node, outer)
        process_nodes(node.body.expressions, outer)
        process_nodes(node.patterns, outer)
        process_node(node.guard, outer) if node.guard
      end

      def on_if(node, outer)
        node.conditions.each do |cond|
          process_node(cond.condition, outer)
          process_node(cond.body, outer)
        end

        process_node(node.else_body, outer) if node.else_body
      end

      def on_and(node, outer)
        process_node(node.left, outer)
        process_node(node.right, outer)
      end

      def on_or(node, outer)
        process_node(node.left, outer)
        process_node(node.right, outer)
      end

      def on_not(node, outer)
        process_node(node.expression, outer)
      end

      def on_loop(node, outer)
        process_node(node.body, outer)
      end

      def on_method(node, _)
        node.body.locals = SymbolTable.new

        process_nodes(node.arguments, node.body)
        process_nodes(node.body.expressions, node.body)
      end

      def on_send(node, outer)
        process_nodes(node.arguments, outer)
        process_node(node.receiver, outer) if node.receiver
      end

      def on_raw_instruction(node, outer)
        process_nodes(node.arguments, outer)
      end

      def on_node_with_body(node, *)
        process_node(node.body, node.body)
      end

      alias on_object on_node_with_body
      alias on_trait on_node_with_body
      alias on_trait_implementation on_node_with_body
      alias on_reopen_object on_node_with_body
      alias on_required_method on_node_with_body

      def on_try(node, outer)
        process_node(node.expression, outer)

        return unless node.else_body

        process_node(node.else_body, outer)

        node.else_body.locals.parent = outer.locals
      end

      def on_node_with_value(node, outer)
        process_node(node.value, outer) if node.value
      end

      alias on_throw on_node_with_value
      alias on_return on_node_with_value
      alias on_define_variable on_node_with_value
      alias on_define_variable_with_explicit_type on_node_with_value
      alias on_reassign_variable on_node_with_value
      alias on_keyword_argument on_node_with_value
      alias on_destructure_array on_node_with_value

      def on_type_cast(node, outer)
        process_node(node.expression, outer)
      end

      def on_new_instance(node, outer)
        node.attributes.each do |attr|
          process_node(attr.value, outer)
        end
      end

      def on_template_string(node, outer)
        node.members.each do |node|
          process_node(node, outer)
        end
      end
    end
  end
end
