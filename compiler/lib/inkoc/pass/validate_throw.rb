# frozen_string_literal: true

module Inkoc
  module Pass
    class ValidateThrow
      include VisitorMethods

      def initialize(compiler, mod)
        @module = mod
        @state = compiler.state
        @block_nesting = []
      end

      def diagnostics
        @state.diagnostics
      end

      def run(ast)
        process_node(ast, @module.body.type)

        [ast]
      end

      def inside_block(block)
        @block_nesting << block

        yield

        @block_nesting.pop
      end

      def every_nested_block
        @block_nesting.reverse_each do |block|
          yield block
        end
      end

      def on_block(node, *)
        inside_block(node.block_type) do
          error_for_missing_throw_in_block(node, node.block_type)
        end
      end
      alias on_lambda on_block

      def on_body(node, block_type)
        process_nodes(node.expressions, block_type)
      end

      def on_define_variable(node, block_type)
        process_node(node.value, block_type)
      end
      alias on_define_variable_with_explicit_type on_define_variable

      def on_destructure_array(node, block_type)
        process_node(node.value, block_type)
      end

      def on_keyword_argument(node, block_type)
        process_node(node.value, block_type)
      end

      def on_method(node, *)
        inside_block(node.block_type) do
          error_for_missing_throw_in_block(node, node.block_type)
        end
      end

      def on_node_with_body(node, *)
        process_node(node.body, node.block_type)
      end

      alias on_object on_node_with_body
      alias on_trait on_node_with_body
      alias on_trait_implementation on_node_with_body
      alias on_reopen_object on_node_with_body

      def on_raw_instruction(node, block_type)
        process_nodes(node.arguments, block_type)
      end

      def on_reassign_variable(node, block_type)
        process_node(node.value, block_type)
      end

      def on_return(node, block_type)
        process_node(node.value, block_type) if node.value
      end

      def on_send(node, block_type, try: false)
        error_for_missing_try(node, try: try)

        process_node(node.receiver, block_type) if node.receiver
        process_nodes(node.arguments, block_type)
      end

      def on_identifier(node, block_type, try: false)
        error_for_missing_try(node, try: try)
      end

      def on_match(node, block_type)
        process_nodes(node.arms, block_type)
        process_node(node.match_else, block_type) if node.match_else
      end

      def on_match_else(node, block_type)
        inside_block(block_type) do
          process_node(node.body, block_type)
        end
      end

      def on_match_type(node, block_type)
        inside_block(block_type) do
          process_nodes(node.body.expressions, block_type)
        end

        process_node(node.guard, block_type) if node.guard
      end

      def on_match_expression(node, block_type)
        inside_block(block_type) do
          process_nodes(node.body.expressions, block_type)
        end

        process_node(node.guard, block_type) if node.guard
      end

      def on_if(node, block_type)
        node.conditions.each do |cond|
          process_node(cond.condition, block_type)
          process_node(cond.body, block_type)
        end

        process_node(node.else_body, block_type) if node.else_body
      end

      def on_and(node, block_type)
        process_node(node.left, block_type)
        process_node(node.right, block_type)
      end

      def on_or(node, block_type)
        process_node(node.left, block_type)
        process_node(node.right, block_type)
      end

      def on_not(node, block_type)
        process_node(node.expression, block_type)
      end

      def on_loop(node, block_type)
        process_node(node.body, block_type)
      end

      def on_group(node, block_type)
        process_nodes(node.expressions, block_type)
        node
      end

      def on_throw(node, block_type, try: false)
        process_node(node.value, block_type)

        thrown = node.value.type
        block = throw_block_scope

        return if try

        unless block
          diagnostics.throw_at_top_level_error(thrown, node.location)
          return
        end

        unless block.throw_type
          diagnostics.throw_without_throw_defined_error(thrown, node.location)
          return
        end

        track_throw_type(thrown, block, node.location)
      end

      def on_try(node, block_type)
        loc = node.location

        case node.expression
        when AST::Identifier
          on_identifier(node.expression, block_type, try: true)
        when AST::Send
          on_send(node.expression, block_type, try: true)
        when AST::Try
          on_try(node.expression, block_type, try: true)
        else
          process_node(node.expression, block_type)
        end

        process_node(node.else_body, block_type)

        return if node.explicit_block_for_else_body?

        track_in = throw_block_scope

        if track_in == @module.body.type || track_in.nil?
          diagnostics.throw_at_top_level_error(node.throw_type, loc)
          return
        else
          error_for_undefined_throw(node.throw_type, track_in, loc)
        end

        track_throw_type(node.throw_type, track_in, node.location)
      end

      def track_throw_type(thrown, block_type, location)
        expected = block_type.throw_type

        block_type.thrown_types << thrown if thrown

        if thrown && expected && !thrown.type_compatible?(expected, @state)
          diagnostics.type_error(expected, thrown, location)
          false
        else
          true
        end
      end

      def on_type_cast(node, block_type)
        process_node(node.expression, block_type)
      end

      def on_new_instance(node, block_type)
        node.attributes.each do |attr|
          process_node(attr.value, block_type)
        end
      end

      def error_for_missing_throw_in_block(node, block_type)
        process_nodes(node.arguments, block_type)
        process_node(node.body, block_type)

        expected = block_type.throw_type

        return if block_type.thrown_types.any? || !expected
        return if expected.never?

        diagnostics.missing_throw_error(expected, node.location)
      end

      def error_for_missing_try(node, try: false)
        return if try
        return unless (throw_type = node.throw_type)
        return if throw_type.never?

        # A generator itself won't throw until its resumed, so a `try` is not
        # needed when creating the generator.
        return if node.block_type.yield_type

        diagnostics.missing_try_error(throw_type, node.location)
      end

      def error_for_undefined_throw(throw_type, block_type, location)
        return if block_type.throw_type
        return unless throw_type

        diagnostics.throw_without_throw_defined_error(throw_type, location)
      end

      def throw_block_scope
        @block_nesting.last
      end

      def inspect
        '#<Pass::ValidateThrow>'
      end
    end
  end
end
